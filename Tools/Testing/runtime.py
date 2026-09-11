"""Owned native process lifecycle and local protocol. No global player/editor state."""
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
PRODUCTS = {'cave': 'Cave', 'sanctuary': 'Sanctuary', 'engine': 'Soundstage'}


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
    temporary.replace(path)


def source_digest():
    digest = hashlib.sha256()
    paths = [ROOT / 'Package.swift']
    for folder in ['Engine', 'Games', 'Tools', 'scripts', 'Testing']:
        paths += [p for p in (ROOT / folder).rglob('*') if p.is_file() and (p.suffix in {'.swift', '.metal', '.json', '.cpp', '.h', '.py'} or (folder in ['scripts','Testing'] and not p.suffix)) and '__pycache__' not in p.parts and 'Baselines' not in p.parts]
    for path in sorted(paths):
        digest.update(str(path.relative_to(ROOT)).encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()


def machine():
    def sysctl(key):
        return subprocess.check_output(['sysctl', '-n', key], text=True).strip()
    return dict(os=platform.mac_ver()[0], architecture=platform.machine(), chip=sysctl('machdep.cpu.brand_string'), memory=int(sysctl('hw.memsize')))


def sandbox(destination, pinned=None):
    """Copy only authored inputs. Runtime compilation can read shared engine source."""
    workspace = destination / 'workspace'
    workspace.mkdir(parents=True, exist_ok=True)
    for name in ['Engine', 'Tools', 'docs', 'scripts']:
        (workspace / name).symlink_to(ROOT / name, target_is_directory=True)
    source = pinned or ROOT
    for folder in (source / 'Games').glob('*/Authoring'):
        shutil.copytree(folder, workspace / folder.relative_to(source))
    return workspace


@contextlib.contextmanager
def gpu_lease():
    path = ROOT / '.build/testing-gpu.lock'
    path.parent.mkdir(exist_ok=True)
    with path.open('w') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('Another harness owns the GPU; finish that run first')
        processes = subprocess.check_output(['ps', '-axo', 'pid=,comm='], text=True)
        others = [line.strip() for line in processes.splitlines() if any('/Contents/MacOS/' + p in line for p in PRODUCTS.values())]
        if others:
            raise RuntimeError('Close other Wrela render windows before an isolated GPU run: ' + '; '.join(others))
        yield


class App:
    def __init__(self, owner, directory, pinned=None):
        self.owner = owner
        self.product = PRODUCTS[owner]
        self.directory = directory
        directory.mkdir(parents=True, exist_ok=True)
        self.workspace = sandbox(directory, pinned)
        self.root = self.workspace / '.soundstage' if owner == 'engine' else directory / 'runtime'
        self.log = (directory / 'app.log').open('w')
        env = dict(os.environ, WRELA_WORKSPACE=str(self.workspace), SANCTUARY_WORKSPACE=str(self.workspace), WRELA_DATA_ROOT=str(self.root), WRELA_CONTROL_ROOT=str(self.root), WRELA_TESTING='1')
        binary = ROOT / '.build' / (self.product + '.app') / 'Contents/MacOS' / self.product
        self.process = subprocess.Popen([str(binary)], cwd=self.workspace, env=env, stdout=self.log, stderr=subprocess.STDOUT)
        write_json(directory / 'session.json', dict(pid=self.process.pid, owner=owner, controlRoot=str(self.root), workspace=str(self.workspace)))
        try:
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                if self.process.poll() is not None:
                    raise RuntimeError('App exited during startup; inspect ' + str(directory / 'app.log'))
                status = self.root / 'status.json'
                if status.exists():
                    state = json.loads(status.read_text())
                    if state.get('sessionPID') == self.process.pid and not state.get('stopped'):
                        self.command('pause', value=True)
                        return
                time.sleep(.1)
            raise RuntimeError('App startup timed out; inspect ' + str(directory / 'app.log'))
        except BaseException:
            self.close()
            raise

    def command(self, action, **values):
        request = dict(id=str(uuid.uuid4()), action=action, **values)
        write_json(self.root / 'inbox' / (request['id'] + '.json'), request)
        response = self.root / 'outbox' / (request['id'] + '.json')
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError('Owned app exited while handling ' + action)
            if response.exists():
                result = json.loads(response.read_text())
                if not result.get('ok'):
                    raise RuntimeError(result.get('error', 'Command rejected'))
                return result
            time.sleep(.025)
        raise RuntimeError('Timed out waiting for ' + action + '; see ' + str(self.directory / 'app.log'))

    def close(self):
        if self.process.poll() is None:
            # SIGTERM targets only the Popen child, never a process discovered by name.
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        self.log.close()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()

@contextlib.contextmanager
def performance_lease():
    """All harness CPU and GPU measurements are mutually exclusive."""
    path = ROOT / '.build/testing-performance.lock'
    with path.open('w') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('Another performance run is active; measure serially')
        yield
