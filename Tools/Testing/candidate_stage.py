#!/usr/bin/env python3
"""Build one prepared candidate through a serialized, reusable SwiftPM workspace."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import stat
import subprocess
import time
import uuid

from runtime import (
    Candidate, _tree, _tree_without_build, build_source_digest, file_digest,
    harness_source_digest, write_json,
)


def tree_digest(tree):
    encoded = json.dumps(tree, sort_keys=True, separators=(',', ':')).encode()
    return hashlib.sha256(encoded).hexdigest()


def remove_path(path):
    if path.is_symlink() or path.is_file():
        path.unlink()
    elif path.exists():
        shutil.rmtree(path)


def mirror_tree(source, destination):
    """Mirror source while retaining destination mtimes for byte-identical files."""
    source_entries = _tree(source)
    if destination.exists() or destination.is_symlink():
        destination_entries = _tree(destination)
    else:
        destination.mkdir(parents=True)
        destination_entries = {}
    removed = copied = retained = 0
    for relative in sorted(set(destination_entries) - set(source_entries),
                           key=lambda value: (len(Path(value).parts), value), reverse=True):
        remove_path(destination / relative)
        removed += 1
    for path in sorted(source.rglob('*'), key=lambda item: item.relative_to(source).as_posix()):
        if path.is_dir() and not path.is_symlink():
            (destination / path.relative_to(source)).mkdir(parents=True, exist_ok=True)
    for relative, identity in source_entries.items():
        source_path = source / relative
        destination_path = destination / relative
        destination_path.parent.mkdir(parents=True, exist_ok=True)
        if destination_entries.get(relative) == identity:
            if source_path.is_file() and not source_path.is_symlink():
                source_mode = stat.S_IMODE(source_path.stat().st_mode)
                if stat.S_IMODE(destination_path.stat().st_mode) != source_mode:
                    destination_path.chmod(source_mode)
            retained += 1
            continue
        remove_path(destination_path)
        if source_path.is_symlink():
            destination_path.symlink_to(os.readlink(source_path),
                                        target_is_directory=source_path.resolve().is_dir())
        else:
            shutil.copy2(source_path, destination_path)
            # A copied checkout can retain an older source timestamp. Bump only changed
            # files so SwiftPM observes the edit while unchanged inputs keep warm mtimes.
            os.utime(destination_path, None)
        copied += 1
    for path in sorted(destination.rglob('*'),
                       key=lambda item: len(item.relative_to(destination).parts), reverse=True):
        if path.is_dir() and not path.is_symlink():
            try:
                path.rmdir()
            except OSError:
                pass
    return dict(copied=copied, retained=retained, removed=removed)


class ReusableCandidateBuild:
    def __init__(
        self, prepared, stage, *, test_filter=None, command_runner=subprocess.run,
        verify_codesign=True
    ):
        self.prepared = Path(prepared).resolve()
        raw_stage = Path(stage)
        if not raw_stage.is_absolute():
            raise RuntimeError('Reusable candidate build stage must be an absolute path')
        if raw_stage.is_symlink():
            raise RuntimeError('Reusable candidate build stage cannot be a symlink')
        self.stage = raw_stage.resolve()
        for outer, inner in [(self.prepared, self.stage), (self.stage, self.prepared)]:
            try:
                inner.relative_to(outer)
            except ValueError:
                pass
            else:
                raise RuntimeError('Reusable build stage and prepared candidate must be separate trees')
        self.command_runner = command_runner
        self.verify_codesign = verify_codesign
        if test_filter is not None and (
            not test_filter.strip() or len(test_filter) > 512 or '\n' in test_filter
        ):
            raise RuntimeError('Reusable candidate test filter is invalid')
        self.test_filter = test_filter
        self.preparation = None

    def _raise_logged_failure(self, phase, log, error):
        receipt_path = log.parent / 'failure.json'
        receipt = dict(
            version=1, passed=False, candidateID=self.prepared.name, stage=str(self.stage),
            phase=phase, failedAt=time.time(), log=str(log),
            errorType=type(error).__name__, error=str(error),
        )
        if isinstance(error, subprocess.CalledProcessError):
            receipt['returnCode'] = error.returncode
            receipt['command'] = list(error.cmd)
        write_json(receipt_path, receipt)
        raise RuntimeError(
            'Reusable candidate build failed during ' + phase + '; inspect ' + str(log)
            + '; failure receipt ' + str(receipt_path)) from error

    def verify_prepared(self):
        current = Candidate.verify_prepared(self.prepared)
        if self.preparation is not None and current != self.preparation:
            raise RuntimeError('Prepared candidate metadata changed during reusable build')
        return current

    def synchronize(self):
        self.preparation = Candidate.verify_prepared(self.prepared)
        if self.stage.exists() and (self.stage.is_symlink() or not self.stage.is_dir()):
            raise RuntimeError('Reusable candidate build stage is not a real directory')
        self.stage.mkdir(parents=True, exist_ok=True)
        changes = {}
        for name in Candidate.input_roots:
            changes[name] = mirror_tree(self.prepared / name, self.stage / name)
        package = self.stage / 'Package.swift'
        source_package = self.prepared / 'Package.swift'
        if not package.is_file() or package.is_symlink() or package.read_bytes() != source_package.read_bytes():
            remove_path(package)
            shutil.copy2(source_package, package)
            os.utime(package, None)
            changes['Package.swift'] = 'copied'
        else:
            changes['Package.swift'] = 'retained'
        self.verify_stage()
        write_json(self.stage / '.candidate-stage-cache.json', dict(
            version=1, synchronizedAt=time.time(), candidateID=self.prepared.name,
            buildSourceDigest=self.preparation['buildSourceDigest'],
            harnessSourceDigest=self.preparation['harnessSourceDigest'], changes=changes,
        ))
        return changes

    def verify_stage(self):
        preparation = self.verify_prepared()
        if _tree_without_build(self.stage) != preparation['workspaceFiles'] \
          or build_source_digest(self.stage) != preparation['buildSourceDigest'] \
          or harness_source_digest(self.stage) != preparation['harnessSourceDigest']:
            raise RuntimeError('Reusable build stage source drifted from prepared candidate')

    def _build_product(self, product, log):
        env = dict(
            os.environ, WRELA_BUNDLE_WORKSPACE=str(self.prepared),
            WRELA_BUILD_REVISION=self.preparation.get('revision', 'unknown'),
            PYTHONDONTWRITEBYTECODE='1',
        )
        try:
            with log.open('w') as output:
                self.command_runner(
                    [str(self.stage / 'scripts/build'), product], cwd=self.stage, env=env,
                    stdout=output, stderr=subprocess.STDOUT, check=True)
        except Exception as error:
            self._raise_logged_failure('building ' + product, log, error)

    def _run_focused_tests(self, log):
        before = tree_digest(_tree_without_build(self.stage))
        started = time.time()
        try:
            with log.open('w') as output:
                self.command_runner(
                    ['swift', 'test', '--filter', self.test_filter], cwd=self.stage,
                    env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'),
                    stdout=output, stderr=subprocess.STDOUT, check=True)
        except Exception as error:
            self._raise_logged_failure('focused Swift tests', log, error)
        self.verify_stage()
        after = tree_digest(_tree_without_build(self.stage))
        if before != after:
            raise RuntimeError('Focused tests changed reusable stage source inputs')
        return dict(
            filter=self.test_filter, log=str(log), startedAt=started, completedAt=time.time(),
            stageSourceBeforeSHA256=before, stageSourceAfterSHA256=after, passed=True)

    def _stage_outputs(self, products):
        outputs = {}
        for product in products:
            app = self.stage / '.build' / (product + '.app')
            info_path = app / 'Contents/Info.plist'
            binary = app / 'Contents/MacOS' / product
            if not info_path.is_file() or not binary.is_file():
                raise RuntimeError('Reusable stage did not produce complete ' + product + '.app')
            info = plistlib.loads(info_path.read_bytes())
            if info.get('SanctuarySourceDigest') != self.preparation['buildSourceDigest']:
                raise RuntimeError('Reusable staged app/source digest mismatch: ' + product)
            if info.get('SanctuaryWorkspace') != str(self.prepared):
                raise RuntimeError('Reusable staged app has the wrong final workspace: ' + product)
            if self.verify_codesign:
                self.command_runner(
                    ['codesign', '--verify', '--deep', '--strict', str(app)], check=True,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            outputs[product] = dict(
                binarySHA256=file_digest(binary), files=_tree(app),
                resources=_tree(app / 'Contents/Resources'))
        runner = self.stage / '.build/release/WrelaTest'
        if runner.is_symlink() or not runner.is_file():
            raise RuntimeError('Reusable stage did not produce WrelaTest')
        return outputs, runner

    def build(self):
        changes = self.synchronize()
        stage_before = tree_digest(_tree_without_build(self.stage))
        products = Candidate._validated_products(self.preparation['requestedProducts'])
        candidate_build = self.prepared / '.build'
        receipt_path = self.prepared / Candidate.build_receipt_name
        installing_path = self.prepared / Candidate.build_installing_name
        destinations = [candidate_build / (product + '.app') for product in products]
        destinations.append(candidate_build / 'release/WrelaTest')
        if receipt_path.exists() or installing_path.exists() or any(path.exists() for path in destinations):
            raise RuntimeError('Prepared candidate already contains reusable or cold build outputs')
        logs = candidate_build / 'candidate-stage-logs' / (
            time.strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:6])
        logs.mkdir(parents=True, exist_ok=False)
        started = time.time()
        focused_tests = None
        if self.test_filter is not None:
            focused_tests = self._run_focused_tests(logs / 'focused-swift-tests.log')
        for product in products:
            remove_path(self.stage / '.build' / (product + '.app'))
            self._build_product(product, logs / (product + '.log'))
            self.verify_stage()
        self._build_product('WrelaTest', logs / 'WrelaTest.log')
        self.verify_stage()
        stage_after = tree_digest(_tree_without_build(self.stage))
        if stage_after != stage_before:
            raise RuntimeError('Reusable build changed its staged source inputs')
        staged_products, staged_runner = self._stage_outputs(products)
        receipt = dict(
            version=1, passed=True, candidateID=self.prepared.name, stage=str(self.stage),
            startedAt=started, completedAt=time.time(), requestedProducts=list(products),
            buildSourceDigest=self.preparation['buildSourceDigest'],
            harnessSourceDigest=self.preparation['harnessSourceDigest'],
            sourceTreeSHA256=tree_digest(self.preparation['workspaceFiles']),
            stageSourceBeforeBuildSHA256=stage_before,
            stageSourceAfterBuildSHA256=stage_after,
            synchronizedFiles=changes,
            revision=self.preparation.get('revision', 'unknown'),
            logs={name: str(logs / (name + '.log')) for name in [*products, 'WrelaTest']},
        )
        if focused_tests is not None:
            receipt['focusedTests'] = focused_tests
        if any(path.exists() for path in destinations):
            raise RuntimeError('Prepared candidate gained build outputs during reusable build')
        write_json(installing_path, dict(receipt, passed=False, phase='installing'))
        temporary = candidate_build / ('.candidate-stage-copy-' + uuid.uuid4().hex[:8])
        try:
            temporary.mkdir()
            for product in products:
                shutil.copytree(
                    self.stage / '.build' / (product + '.app'), temporary / (product + '.app'),
                    symlinks=True, copy_function=shutil.copy2)
            shutil.copy2(staged_runner, temporary / 'WrelaTest')
            self.verify_stage()
            installed_products = {}
            for product in products:
                copied = temporary / (product + '.app')
                if _tree(copied) != staged_products[product]['files']:
                    raise RuntimeError('Reusable app copy changed bytes: ' + product)
                installed_products[product] = staged_products[product]
            runner_sha = file_digest(temporary / 'WrelaTest')
            if runner_sha != file_digest(staged_runner):
                raise RuntimeError('Reusable WrelaTest copy changed bytes')
            self.verify_prepared()
            for product in products:
                (temporary / (product + '.app')).replace(candidate_build / (product + '.app'))
            release = candidate_build / 'release'
            if release.is_symlink():
                raise RuntimeError('Prepared candidate release output cannot be a symlink')
            release.mkdir()
            (temporary / 'WrelaTest').replace(release / 'WrelaTest')
            for product in products:
                installed_app = candidate_build / (product + '.app')
                if _tree(installed_app) != staged_products[product]['files']:
                    raise RuntimeError('Installed reusable app changed bytes: ' + product)
                if self.verify_codesign:
                    self.command_runner(
                        ['codesign', '--verify', '--deep', '--strict', str(installed_app)], check=True,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if file_digest(release / 'WrelaTest') != runner_sha:
                raise RuntimeError('Installed reusable WrelaTest changed bytes')
            receipt['installedProducts'] = installed_products
            receipt['executables'] = {'WrelaTest': dict(
                path='.build/release/WrelaTest', binarySHA256=runner_sha)}
            self.verify_prepared()
            write_json(receipt_path, receipt)
            installing_path.unlink()
            return receipt
        finally:
            shutil.rmtree(temporary, ignore_errors=True)

    def run(self):
        lock = self.stage.parent / (self.stage.name + '.lock')
        lock.parent.mkdir(parents=True, exist_ok=True)
        with lock.open('w') as file:
            fcntl.flock(file, fcntl.LOCK_EX)
            return self.build()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prepared', type=Path)
    parser.add_argument('stage', type=Path)
    parser.add_argument('--test-filter', help='Run focused Swift tests in the stage before copying outputs')
    args = parser.parse_args()
    try:
        receipt = ReusableCandidateBuild(
            args.prepared, args.stage, test_filter=args.test_filter).run()
    except RuntimeError as error:
        parser.exit(1, 'error: ' + str(error) + '\n')
    print('Reusable candidate build ready to seal: ' + str(Path(args.prepared).resolve()))
    print(json.dumps(receipt, indent=2, sort_keys=True))


if __name__ == '__main__':
    main()
