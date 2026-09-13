"""Owned native process lifecycle and local protocol. No global player/editor state."""
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import time
import uuid

from provenance import build_source_digest, harness_source_digest

ROOT = Path(__file__).resolve().parents[2]
PRODUCTS = {'cave': 'Cave', 'sanctuary': 'Sanctuary', 'engine': 'Soundstage'}


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
    temporary.replace(path)


def source_digest():
    return harness_source_digest(ROOT)


def file_digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _tree(root):
    """Hash an immutable tree, preserving and containing app-internal links."""
    if root.is_symlink() or not root.is_dir():
        raise RuntimeError('Candidate root is missing or linked: ' + str(root))
    entries = {}
    for path in sorted(root.rglob('*'), key=lambda item: item.relative_to(root).as_posix()):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            target = os.readlink(path)
            resolved = path.parent.joinpath(target).resolve()
            try:
                resolved.relative_to(root.resolve())
            except ValueError:
                raise RuntimeError('Candidate link escapes its snapshot: ' + relative)
            entries[relative] = 'link:' + target
        elif path.is_file():
            entries[relative] = 'file:' + file_digest(path)
        elif not path.is_dir():
            raise RuntimeError('Unsupported candidate input: ' + relative)
    return entries


def _copy_tree(source, destination, preflight=True):
    if preflight:
        _tree(source)  # preflight links before copytree could follow one into live inputs.
    shutil.copytree(
        source, destination, symlinks=True,
        ignore=shutil.ignore_patterns('__pycache__', '*.pyc', '.build', '.git'))


class Candidate:
    """A frozen source snapshot and matching already-built native app bundles."""
    version = 2
    preparation_version = 1
    preparation_name = '.candidate-preparation.json'
    build_receipt_name = '.candidate-build-stage.json'
    build_installing_name = '.candidate-build-installing.json'
    input_roots = ('Engine', 'Games', 'Tools', 'scripts', 'Testing', 'docs')

    def __init__(self, root):
        self.root = Path(root).resolve()
        self.manifest_path = self.root / 'candidate.json'
        if not self.manifest_path.is_file() or self.manifest_path.is_symlink():
            raise RuntimeError('Candidate manifest is missing or linked: ' + str(self.manifest_path))
        self.manifest = json.loads(self.manifest_path.read_text())
        if self.manifest.get('version') != self.version:
            raise RuntimeError('Unsupported candidate manifest; capture a new candidate with scripts/test candidate')
        self.verify()

    @property
    def workspace(self):
        return self.root

    @property
    def identifier(self):
        return self.manifest['id']

    @property
    def source_digest(self):
        return self.manifest['harnessSourceDigest']

    @classmethod
    def _validated_products(cls, products):
        products = tuple(sorted(set(products)))
        if not products or any(product not in PRODUCTS.values() for product in products):
            raise RuntimeError('Candidate needs one known app product')
        return products

    @classmethod
    def prepare(cls, destination, products):
        """Copy coherent source quickly; products are built later inside this snapshot."""
        destination = Path(destination).resolve()
        if destination.exists():
            raise RuntimeError('Candidate destination already exists: ' + str(destination))
        products = cls._validated_products(products)
        before = _tree_without_build(ROOT)
        copied = destination.with_name(destination.name + '.copying-' + uuid.uuid4().hex[:8])
        try:
            copied.mkdir(parents=True)
            for name in cls.input_roots:
                # The coherent `before` tree already preflighted every input link.
                _copy_tree(ROOT / name, copied / name, preflight=False)
            shutil.copy2(ROOT / 'Package.swift', copied / 'Package.swift')
            workspace = _tree_without_build(copied)
            preparation = dict(
                version=cls.preparation_version, id=destination.name, preparedAt=time.time(),
                requestedProducts=list(products), harnessSourceDigest=harness_source_digest(copied),
                buildSourceDigest=build_source_digest(copied), workspaceFiles=workspace,
                revision=_source_revision(ROOT),
            )
            if before != workspace:
                raise RuntimeError('Source changed while preparing candidate; repeat the brief copy barrier')
            write_json(copied / cls.preparation_name, preparation)
            copied.replace(destination)
            return destination
        except BaseException:
            shutil.rmtree(copied, ignore_errors=True)
            raise

    @classmethod
    def verify_prepared(cls, root):
        """Return preparation metadata only while its immutable source is coherent."""
        root = Path(root).resolve()
        preparation_path = root / cls.preparation_name
        if not preparation_path.is_file() or preparation_path.is_symlink():
            raise RuntimeError('Candidate preparation is missing or linked: ' + str(preparation_path))
        if (root / 'candidate.json').exists():
            raise RuntimeError('Candidate is already sealed: ' + str(root))
        preparation = json.loads(preparation_path.read_text())
        cls._validated_products(preparation.get('requestedProducts', []))
        if preparation.get('version') != cls.preparation_version or preparation.get('id') != root.name:
            raise RuntimeError('Candidate preparation metadata is invalid')
        workspace = _tree_without_build(root)
        if workspace != preparation.get('workspaceFiles') \
          or harness_source_digest(root) != preparation.get('harnessSourceDigest') \
          or build_source_digest(root) != preparation.get('buildSourceDigest'):
            raise RuntimeError('Prepared candidate source snapshot changed before sealing')
        return preparation

    @classmethod
    def seal(cls, root):
        """Verify prepared source and matching in-snapshot builds, then publish a candidate."""
        root = Path(root).resolve()
        preparation_path = root / cls.preparation_name
        preparation = cls.verify_prepared(root)
        products = cls._validated_products(preparation.get('requestedProducts', []))
        if (root / cls.build_installing_name).exists():
            raise RuntimeError('Reusable candidate build installation did not complete')
        expected = preparation['buildSourceDigest']
        apps = {}
        for product in products:
            bundle = root / '.build' / (product + '.app')
            if not bundle.is_dir():
                raise RuntimeError('Snapshot build is missing: ' + str(bundle))
            info_path = bundle / 'Contents/Info.plist'
            binary = bundle / 'Contents/MacOS' / product
            if not info_path.is_file() or info_path.is_symlink() or not binary.is_file() or binary.is_symlink():
                raise RuntimeError('Snapshot app is incomplete or linked: ' + product)
            info = plistlib.loads(info_path.read_bytes())
            if info.get('SanctuarySourceDigest') != expected:
                raise RuntimeError('Candidate app/source digest mismatch: ' + product)
            apps[product] = dict(
                sourceDigest=info.get('SanctuarySourceDigest'),
                revision=info.get('SanctuaryRevision'),
                binarySHA256=file_digest(binary), files=_tree(bundle),
            )
        executables = {}
        build_staging = None
        build_receipt_path = root / cls.build_receipt_name
        if build_receipt_path.exists():
            if build_receipt_path.is_symlink() or not build_receipt_path.is_file():
                raise RuntimeError('Reusable candidate build receipt is invalid')
            build_staging = json.loads(build_receipt_path.read_text())
            if build_staging.get('version') != 1 or not build_staging.get('passed') \
              or build_staging.get('candidateID') != root.name \
              or build_staging.get('buildSourceDigest') != expected \
              or build_staging.get('harnessSourceDigest') != preparation['harnessSourceDigest'] \
              or tuple(build_staging.get('requestedProducts', [])) != products:
                raise RuntimeError('Reusable candidate build receipt does not match prepared source')
            installed = build_staging.get('installedProducts', {})
            for product in products:
                if installed.get(product, {}).get('files') != apps[product]['files']:
                    raise RuntimeError('Reusable staged app changed before sealing: ' + product)
            runner = root / '.build/release/WrelaTest'
            recorded_runner = build_staging.get('executables', {}).get('WrelaTest', {})
            if runner.is_symlink() or not runner.is_file() \
              or file_digest(runner) != recorded_runner.get('binarySHA256'):
                raise RuntimeError('Reusable staged WrelaTest changed before sealing')
            executables['WrelaTest'] = dict(
                path='.build/release/WrelaTest', binarySHA256=file_digest(runner))
        manifest = dict(
            version=cls.version, id=root.name, preparedAt=preparation['preparedAt'], sealedAt=time.time(),
            harnessSourceDigest=preparation['harnessSourceDigest'], buildSourceDigest=expected,
            workspaceFiles=preparation['workspaceFiles'], products=apps,
        )
        if executables:
            manifest['executables'] = executables
            manifest['buildStaging'] = build_staging
        preparation_path.unlink()
        build_receipt_path.unlink(missing_ok=True)
        write_json(root / 'candidate.json', manifest)
        return cls(root)

    @classmethod
    def capture(cls, destination, products):
        """Compatibility helper for callers that already have matching live app bundles."""
        destination = cls.prepare(destination, products)
        try:
            for product in cls._validated_products(products):
                source = ROOT / '.build' / (product + '.app')
                if not source.is_dir():
                    raise RuntimeError('Built app is missing: ' + str(source))
                _copy_tree(source, destination / '.build' / source.name)
            return cls.seal(destination)
        except BaseException:
            shutil.rmtree(destination, ignore_errors=True)
            raise

    def app(self, owner):
        product = PRODUCTS[owner]
        if product not in self.manifest['products']:
            raise RuntimeError('Candidate does not contain ' + product + '.app')
        return self.root / '.build' / (product + '.app')

    def metadata(self):
        metadata = dict(
            id=self.identifier, harnessSourceDigest=self.manifest['harnessSourceDigest'],
            buildSourceDigest=self.manifest['buildSourceDigest'], products={
                product: dict(sourceDigest=value['sourceDigest'], binarySHA256=value['binarySHA256'])
                for product, value in self.manifest['products'].items()
            })
        if self.manifest.get('executables'):
            metadata['executables'] = self.manifest['executables']
        return metadata

    def verify(self):
        if _tree_without_build(self.root) != self.manifest.get('workspaceFiles'):
            raise RuntimeError('Candidate source snapshot changed')
        if harness_source_digest(self.root) != self.manifest.get('harnessSourceDigest'):
            raise RuntimeError('Candidate harness digest changed')
        expected = self.manifest.get('buildSourceDigest')
        if build_source_digest(self.root) != expected:
            raise RuntimeError('Candidate build digest changed')
        for product, recorded in self.manifest.get('products', {}).items():
            bundle = self.root / '.build' / (product + '.app')
            if _tree(bundle) != recorded.get('files'):
                raise RuntimeError('Candidate app bundle changed: ' + product)
            info = plistlib.loads((bundle / 'Contents/Info.plist').read_bytes())
            if info.get('SanctuarySourceDigest') != expected or recorded.get('sourceDigest') != expected:
                raise RuntimeError('Candidate app/source digest mismatch: ' + product)
            if file_digest(bundle / 'Contents/MacOS' / product) != recorded.get('binarySHA256'):
                raise RuntimeError('Candidate app binary changed: ' + product)
        for name, recorded in self.manifest.get('executables', {}).items():
            executable = self.root / recorded.get('path', '')
            if executable.is_symlink() or not executable.is_file() \
              or file_digest(executable) != recorded.get('binarySHA256'):
                raise RuntimeError('Candidate executable changed: ' + name)


def _tree_without_build(root):
    entries = {}
    for name in Candidate.input_roots:
        entries.update({
            name + '/' + path: value for path, value in _tree(root / name).items()
            if not any(part in {'.build', '.git', '__pycache__'} for part in Path(path).parts)
            and not path.endswith('.pyc')
        })
    package = root / 'Package.swift'
    if package.is_symlink() or not package.is_file():
        raise RuntimeError('Candidate Package.swift is missing or linked')
    entries['Package.swift'] = 'file:' + file_digest(package)
    return entries


def _source_revision(root):
    """Human-readable revision; content identity remains the source digests above."""
    try:
        revision = subprocess.check_output(
            ['git', '-C', str(root), 'rev-parse', '--short', 'HEAD'], text=True,
            stderr=subprocess.DEVNULL).strip()
        if subprocess.check_output(
            ['git', '-C', str(root), 'status', '--porcelain'], text=True,
            stderr=subprocess.DEVNULL).strip():
            revision += '-working-tree'
        return revision
    except (OSError, subprocess.CalledProcessError):
        return 'unknown'


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
    def __init__(self, owner, directory, pinned=None, candidate=None):
        self.owner = owner
        self.product = PRODUCTS[owner]
        self.directory = directory
        directory.mkdir(parents=True, exist_ok=True)
        self.candidate = candidate
        if candidate:
            candidate.verify()
            if pinned is not None:
                raise RuntimeError('A candidate already contains pinned authored inputs')
            self.workspace = candidate.workspace
            bundle = candidate.app(owner)
        else:
            self.workspace = sandbox(directory, pinned)
            bundle = ROOT / '.build' / (self.product + '.app')
        self.root = (
            directory / '.soundstage' if candidate and owner == 'engine'
            else self.workspace / '.soundstage' if owner == 'engine' else directory / 'runtime'
        )
        self.env = dict(os.environ, WRELA_WORKSPACE=str(self.workspace), SANCTUARY_WORKSPACE=str(self.workspace), WRELA_DATA_ROOT=str(self.root), WRELA_CONTROL_ROOT=str(self.root), WRELA_TESTING='1', PYTHONDONTWRITEBYTECODE='1')
        if candidate:
            self.env['WRELA_CANDIDATE_ID'] = candidate.identifier
        self.binary = bundle / 'Contents/MacOS' / self.product
        self.process = None
        self.log = None
        self.log_path = None
        self.launches = []
        try:
            self._launch()
        except BaseException:
            self.close()
            raise

    def _launch(self, restarted_from=None):
        if self.candidate:
            self.candidate.verify()
        generation = len(self.launches)
        self.log_path = self.directory / ('app-' + str(generation) + '.log')
        self.log = self.log_path.open('w')
        self.process = subprocess.Popen(
            [str(self.binary)], cwd=self.workspace, env=self.env, stdout=self.log,
            stderr=subprocess.STDOUT)
        launch = dict(
            generation=generation, pid=self.process.pid, owner=self.owner,
            executable=str(self.binary), log=str(self.log_path), controlRoot=str(self.root),
            workspace=str(self.workspace), restartedFromPID=restarted_from,
            candidate=self.candidate.metadata() if self.candidate else None,
        )
        self.launches.append(launch)
        write_json(self.directory / 'processes.json', self.launches)
        write_json(self.directory / 'session.json', launch)
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError('App exited during startup; inspect ' + str(self.log_path))
            status = self.root / 'status.json'
            if status.exists():
                state = json.loads(status.read_text())
                if state.get('sessionPID') == self.process.pid and not state.get('stopped'):
                    if self.candidate and state.get('sourceDigest') != self.candidate.manifest['buildSourceDigest']:
                        raise RuntimeError('Native app reported a source digest outside its candidate')
                    self.command('pause', value=True)
                    return launch
            time.sleep(.1)
        raise RuntimeError('App startup timed out; inspect ' + str(self.log_path))

    def _stop(self):
        if self.process is None:
            if self.log and not self.log.closed:
                self.log.close()
            return
        if self.process.poll() is None:
            # SIGTERM targets only the Popen child, never a process discovered by name.
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        if self.log and not self.log.closed:
            self.log.close()
        if self.log_path and self.log_path.exists():
            shutil.copy2(self.log_path, self.directory / 'app.log')
        self.launches[-1]['exitCode'] = self.process.returncode
        write_json(self.directory / 'processes.json', self.launches)
        self.process = None

    def restart(self):
        """Relaunch this owned bundle against the same isolated workspace and data root."""
        if self.process is None:
            raise RuntimeError('Cannot restart a closed app')
        previous_pid = self.process.pid
        previous_log = str(self.log_path)
        self._stop()
        current = self._launch(restarted_from=previous_pid)
        return dict(previousPID=previous_pid, currentPID=current['pid'],
                    previousProcessLog=previous_log, currentProcessLog=current['log'],
                    processLog=current['log'])

    def command(self, action, **values):
        if self.process is None:
            raise RuntimeError('Owned app is closed')
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
        self._stop()
        if self.candidate:
            self.candidate.verify()

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
