"""Fast tests of report, baseline and exclusivity policies. No native app required."""
import copy
import json
from pathlib import Path
import plistlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from report import report
from runner import (
    canonical_persistent_snapshot, compare_perf, cpu, difference, render_trace,
    wait_for_native_readiness,
)
from runtime import Candidate, performance_lease, write_json
from candidate_stage import ReusableCandidateBuild
from provenance import build_source_digest


class HarnessPolicyTests(unittest.TestCase):
    def candidate_source(self, root, with_app=True):
        for name in ['Engine','Games','Tools','scripts','Testing','docs']:
            (root/name).mkdir(parents=True,exist_ok=True)
        (root/'Package.swift').write_text('// package\n')
        (root/'Engine'/'Core.swift').write_text('enum Core {}\n')
        (root/'Games'/'Game.swift').write_text('enum Game {}\n')
        (root/'Tools'/'Tool.py').write_text('tool = 1\n')
        (root/'scripts'/'validate').write_text('#!/bin/sh\n')
        (root/'Testing'/'policy.json').write_text('{}\n')
        (root/'docs'/'TESTING.md').write_text('test\n')
        if with_app:
            self.candidate_app(root)

    def candidate_app(self, root):
        app=root/'.build/Sanctuary.app/Contents'
        (app/'MacOS').mkdir(parents=True,exist_ok=True)
        (app/'MacOS'/'Sanctuary').write_bytes(b'candidate-native')
        with (app/'Info.plist').open('wb') as file:
            plistlib.dump({
                'CFBundleExecutable':'Sanctuary', 'SanctuarySourceDigest':build_source_digest(root),
                'SanctuaryRevision':'test',
            },file)

    def capture_candidate(self, root, destination):
        with patch('runtime.ROOT', root):
            return Candidate.capture(destination, ['Sanctuary'])

    def test_cpu_uses_verified_candidate_runner_without_building(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            workspace = folder / 'candidate'
            executable = workspace / '.build/release/WrelaTest'
            executable.parent.mkdir(parents=True)
            executable.write_bytes(b'pinned-runner')
            (workspace / 'candidate.json').write_text('{}\n')
            output = folder / 'result'
            output.mkdir()
            verified = object()
            candidate = type('VerifiedCandidate', (), {
                'manifest': {'executables': {'WrelaTest': {'path': '.build/release/WrelaTest'}}},
                'verify': lambda self: verified,
            })()

            def run(invocation, **_):
                destination = Path(invocation[invocation.index('--output') + 1])
                destination.write_text('[{"passed": true}]\n')
                return subprocess.CompletedProcess(invocation, 0)

            with patch('runner.Candidate', return_value=candidate) as load_candidate, \
                 patch('runner.build') as build_runner, patch('runner.subprocess.run', side_effect=run) as launch:
                result = cpu('run', 'sanctuary', output, '--test', 'one', workspace=workspace,
                    digest='a' * 64)

            self.assertEqual(result, [{'passed': True}])
            load_candidate.assert_called_once_with(workspace.resolve())
            build_runner.assert_not_called()
            self.assertEqual(Path(launch.call_args.args[0][0]), executable.resolve())

    def test_cpu_fails_closed_when_sealed_candidate_has_no_runner(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            workspace = folder / 'candidate'
            workspace.mkdir()
            (workspace / 'candidate.json').write_text('{}\n')
            output = folder / 'result'
            output.mkdir()
            candidate = type('VerifiedCandidate', (), {
                'manifest': {'executables': {}},
                'verify': lambda self: None,
            })()

            with patch('runner.Candidate', return_value=candidate), \
                 patch('runner.build') as build_runner, patch('runner.subprocess.run') as launch:
                with self.assertRaisesRegex(RuntimeError, 'does not contain a pinned WrelaTest'):
                    cpu('run', 'sanctuary', output, workspace=workspace, digest='a' * 64)

            build_runner.assert_not_called()
            launch.assert_not_called()

    def fake_stage_build(self, command, *, cwd, env, **_):
        if command[:2]==['swift','test']:
            return
        workspace=Path(cwd);product=command[-1]
        if product=='WrelaTest':
            runner=workspace/'.build/release/WrelaTest';runner.parent.mkdir(parents=True,exist_ok=True)
            runner.write_bytes(b'runner-'+build_source_digest(workspace).encode())
            runner.chmod(0o755)
            return
        app=workspace/'.build'/f'{product}.app/Contents'
        (app/'MacOS').mkdir(parents=True,exist_ok=True)
        (app/'Resources').mkdir(parents=True,exist_ok=True)
        digest=build_source_digest(workspace)
        (app/'MacOS'/product).write_bytes(b'native-'+digest.encode())
        (app/'Resources/Surface.metal').write_bytes(b'resource-'+digest.encode())
        with (app/'Info.plist').open('wb') as file:
            plistlib.dump({
                'CFBundleExecutable':product, 'SanctuarySourceDigest':digest,
                'SanctuaryRevision':env['WRELA_BUILD_REVISION'],
                'SanctuaryWorkspace':env['WRELA_BUNDLE_WORKSPACE'],
            },file)

    def test_candidate_can_build_from_prepared_source_after_live_source_resumes(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'live';root.mkdir();self.candidate_source(root,with_app=False)
            cache=root/'Tools/__pycache__/Tool.pyc';cache.parent.mkdir();cache.write_bytes(b'cache')
            destination=Path(folder)/'candidate'
            with patch('runtime.ROOT', root):
                prepared=Candidate.prepare(destination,['Sanctuary'])
            (root/'Games'/'Game.swift').write_text('enum LiveAuthoringContinues {}\n')
            self.candidate_app(prepared)
            candidate=Candidate.seal(prepared)
            candidate.verify()
            self.assertEqual((prepared/'Games'/'Game.swift').read_text(),'enum Game {}\n')
            self.assertFalse((prepared/'Tools/__pycache__').exists())
            self.assertNotEqual(build_source_digest(root),candidate.manifest['buildSourceDigest'])

    def test_reusable_stage_updates_changed_source_without_touching_old_candidate(self):
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);root=folder/'live';root.mkdir();self.candidate_source(root,with_app=False)
            with patch('runtime.ROOT',root):
                first=Candidate.prepare(folder/'first',['Sanctuary'])
            stage=folder/'stable-stage'
            builder=ReusableCandidateBuild(
                first,stage,command_runner=self.fake_stage_build,verify_codesign=False)
            builder.run();old=Candidate.seal(first)
            old_source=(old.workspace/'Games/Game.swift').read_bytes()
            old_binary=(old.app('sanctuary')/'Contents/MacOS/Sanctuary').read_bytes()
            unchanged_mtime=(stage/'Engine/Core.swift').stat().st_mtime_ns

            (root/'Games/Game.swift').write_text('enum ChangedForSecondCandidate {}\n')
            with patch('runtime.ROOT',root):
                second=Candidate.prepare(folder/'second',['Sanctuary'])
            commands=[]
            def record_build(command, **kwargs):
                commands.append(command)
                return self.fake_stage_build(command,**kwargs)
            second_receipt=ReusableCandidateBuild(
                second,stage,test_filter='SanctuaryContentTests|SanctuaryProjectTests',
                command_runner=record_build,verify_codesign=False).run()
            new=Candidate.seal(second)
            self.assertEqual(commands[0][:2],['swift','test'])
            self.assertEqual([command[-1] for command in commands[1:]],['Sanctuary','WrelaTest'])
            self.assertEqual((stage/'Games/Game.swift').read_text(),'enum ChangedForSecondCandidate {}\n')
            self.assertEqual((stage/'Engine/Core.swift').stat().st_mtime_ns,unchanged_mtime)
            self.assertEqual((old.workspace/'Games/Game.swift').read_bytes(),old_source)
            self.assertEqual((old.app('sanctuary')/'Contents/MacOS/Sanctuary').read_bytes(),old_binary)
            self.assertNotEqual((new.app('sanctuary')/'Contents/MacOS/Sanctuary').read_bytes(),old_binary)
            self.assertTrue(second_receipt['focusedTests']['passed'])
            self.assertEqual(
                second_receipt['focusedTests']['stageSourceBeforeSHA256'],
                second_receipt['focusedTests']['stageSourceAfterSHA256'])
            old.verify();new.verify()

    def test_reusable_stage_rejects_stage_and_prepared_source_drift(self):
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);root=folder/'live';root.mkdir();self.candidate_source(root,with_app=False)
            with patch('runtime.ROOT',root):
                prepared=Candidate.prepare(folder/'prepared',['Sanctuary'])
            builder=ReusableCandidateBuild(
                prepared,folder/'stable-stage',command_runner=self.fake_stage_build,
                verify_codesign=False)
            builder.synchronize()
            (builder.stage/'Games/Game.swift').write_text('enum StageDrift {}\n')
            with self.assertRaisesRegex(RuntimeError,'stage source drifted'):
                builder.verify_stage()
            (prepared/'Games/Game.swift').write_text('enum PreparedDrift {}\n')
            with self.assertRaisesRegex(RuntimeError,'changed before sealing'):
                builder.verify_prepared()

    def test_failed_reusable_build_copies_nothing_and_cannot_seal(self):
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);root=folder/'live';root.mkdir();self.candidate_source(root,with_app=False)
            with patch('runtime.ROOT',root):
                prepared=Candidate.prepare(folder/'prepared',['Sanctuary'])
            def fail_build(*_,**__):
                raise subprocess.CalledProcessError(1,['fake-build'])
            builder=ReusableCandidateBuild(
                prepared,folder/'stable-stage',command_runner=fail_build,verify_codesign=False)
            with self.assertRaisesRegex(RuntimeError, r'inspect .*/Sanctuary\.log'):
                builder.run()
            self.assertFalse((prepared/'.build/Sanctuary.app').exists())
            self.assertFalse((prepared/Candidate.build_receipt_name).exists())
            self.assertFalse((prepared/'candidate.json').exists())
            failure_paths=list((prepared/'.build/candidate-stage-logs').glob('*/failure.json'))
            self.assertEqual(len(failure_paths),1)
            failure=json.loads(failure_paths[0].read_text())
            self.assertEqual(failure['phase'],'building Sanctuary')
            self.assertEqual(failure['log'],str(failure_paths[0].parent/'Sanctuary.log'))
            self.assertEqual(failure['returnCode'],1)
            with self.assertRaisesRegex(RuntimeError,'Snapshot build is missing'):
                Candidate.seal(prepared)

    def test_failed_focused_tests_run_before_release_and_preserve_old_candidate(self):
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);root=folder/'live';root.mkdir();self.candidate_source(root,with_app=False)
            stage=folder/'stable-stage'
            with patch('runtime.ROOT',root):
                first=Candidate.prepare(folder/'first',['Sanctuary'])
            ReusableCandidateBuild(
                first,stage,command_runner=self.fake_stage_build,verify_codesign=False).run()
            old=Candidate.seal(first)
            old_binary=(old.app('sanctuary')/'Contents/MacOS/Sanctuary').read_bytes()
            staged_binary=(stage/'.build/Sanctuary.app/Contents/MacOS/Sanctuary').read_bytes()

            (root/'Games/Game.swift').write_text('enum FocusedFailureCandidate {}\n')
            with patch('runtime.ROOT',root):
                prepared=Candidate.prepare(folder/'prepared',['Sanctuary'])
            commands=[]
            def fail_focused(command, **kwargs):
                commands.append(command)
                if command[:2]==['swift','test']:
                    raise subprocess.CalledProcessError(7,command)
                return self.fake_stage_build(command,**kwargs)
            builder=ReusableCandidateBuild(
                prepared,stage,test_filter='SanctuaryProjectTests',
                command_runner=fail_focused,verify_codesign=False)
            with self.assertRaisesRegex(RuntimeError, r'focused Swift tests; inspect .*/focused-swift-tests\.log'):
                builder.run()
            self.assertEqual(len(commands),1)
            self.assertEqual(commands[0][:2],['swift','test'])

            failure_paths=list((prepared/'.build/candidate-stage-logs').glob('*/failure.json'))
            self.assertEqual(len(failure_paths),1)
            failure=json.loads(failure_paths[0].read_text())
            self.assertEqual(failure['phase'],'focused Swift tests')
            self.assertEqual(failure['log'],str(failure_paths[0].parent/'focused-swift-tests.log'))
            self.assertTrue(Path(failure['log']).is_file())
            self.assertEqual(failure['returnCode'],7)
            self.assertFalse((prepared/'.build/Sanctuary.app').exists())
            self.assertFalse((prepared/Candidate.build_receipt_name).exists())
            self.assertFalse((prepared/'candidate.json').exists())
            self.assertEqual(
                (stage/'.build/Sanctuary.app/Contents/MacOS/Sanctuary').read_bytes(),staged_binary)
            with self.assertRaisesRegex(RuntimeError,'Snapshot build is missing'):
                Candidate.seal(prepared)
            self.assertEqual((old.app('sanctuary')/'Contents/MacOS/Sanctuary').read_bytes(),old_binary)
            old.verify()

    def test_reusable_stage_cli_reports_failures_without_a_python_traceback(self):
        result=subprocess.run(
            ['python3',str(Path(__file__).with_name('candidate_stage.py')),
             '/missing-prepared-candidate','relative-stage'],
            text=True,capture_output=True)
        self.assertEqual(result.returncode,1)
        self.assertIn('error: Reusable candidate build stage must be an absolute path',result.stderr)
        self.assertNotIn('Traceback',result.stderr)

    def test_candidate_rejects_snapshot_mutation_before_and_after_sealing(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'live';root.mkdir();self.candidate_source(root,with_app=False)
            with patch('runtime.ROOT', root):
                prepared=Candidate.prepare(Path(folder)/'before',['Sanctuary'])
            (prepared/'Games'/'Game.swift').write_text('enum MutatedBeforeSeal {}\n')
            self.candidate_app(prepared)
            with self.assertRaisesRegex(RuntimeError,'changed before sealing'):
                Candidate.seal(prepared)

            with patch('runtime.ROOT', root):
                prepared=Candidate.prepare(Path(folder)/'after',['Sanctuary'])
            self.candidate_app(prepared)
            candidate=Candidate.seal(prepared)
            (candidate.workspace/'Games'/'Game.swift').write_text('enum MutatedAfterSeal {}\n')
            with self.assertRaisesRegex(RuntimeError,'snapshot changed'):
                candidate.verify()

    def test_candidate_is_deep_copied_and_ignores_later_live_source_edits(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'live';root.mkdir();self.candidate_source(root)
            candidate=self.capture_candidate(root,Path(folder)/'candidate')
            (root/'Games'/'Game.swift').write_text('enum Changed {}\n')
            candidate.verify()
            self.assertEqual((candidate.workspace/'Games'/'Game.swift').read_text(),'enum Game {}\n')
            self.assertFalse((candidate.workspace/'Engine').is_symlink())

    def test_candidate_rejects_mixed_or_mutated_inputs(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'live';root.mkdir();self.candidate_source(root)
            app=root/'.build/Sanctuary.app/Contents/Info.plist'
            with app.open('wb') as file:
                plistlib.dump({'CFBundleExecutable':'Sanctuary','SanctuarySourceDigest':'wrong'},file)
            with patch('runtime.ROOT', root):
                with self.assertRaisesRegex(RuntimeError,'app/source digest mismatch'):
                    Candidate.capture(Path(folder)/'mixed',['Sanctuary'])
            self.candidate_source(root)
            candidate=self.capture_candidate(root,Path(folder)/'candidate')
            (candidate.workspace/'Games'/'Game.swift').write_text('enum Tampered {}\n')
            with self.assertRaisesRegex(RuntimeError,'snapshot changed'):
                candidate.verify()

    def test_candidate_rejects_a_live_source_link(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'live';root.mkdir();self.candidate_source(root)
            outside=Path(folder)/'outside.swift';outside.write_text('enum Outside {}\n')
            (root/'Engine'/'outside.swift').symlink_to(outside)
            with patch('runtime.ROOT', root):
                with self.assertRaisesRegex(RuntimeError,'escapes its snapshot'):
                    Candidate.capture(Path(folder)/'candidate',['Sanctuary'])

    def test_candidate_copies_and_guards_frozen_baselines(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'live';root.mkdir();self.candidate_source(root)
            baseline=root/'Testing/Baselines/frozen/result.json'
            baseline.parent.mkdir(parents=True)
            baseline.write_text('{"passed": true}\n')
            candidate=self.capture_candidate(root,Path(folder)/'candidate')
            copied=candidate.workspace/'Testing/Baselines/frozen/result.json'
            self.assertEqual(copied.read_text(),'{"passed": true}\n')
            copied.write_text('{"passed": false}\n')
            with self.assertRaisesRegex(RuntimeError,'snapshot changed'):
                candidate.verify()

    def test_soundstage_bridge_prefers_its_isolated_session_root(self):
        source=(Path(__file__).resolve().parents[1]/'SoundstageKit'/'App.swift').read_text()
        start=source.index('private func soundstageSessionRoot()')
        root=source[start:source.index('\n}\n',start)+2]
        self.assertLess(root.index('WRELA_CONTROL_ROOT'),root.index('WRELA_DATA_ROOT'))
        self.assertIn('appendingPathComponent(".soundstage")',root)
        self.assertIn('rootURL = soundstageSessionRoot()',source)
        self.assertIn('soundstageSessionRoot().appendingPathComponent("last-study.json")',source)
        self.assertIn('rootURL.appendingPathComponent("last-study.json")',source)

    def test_difference_decodes_nested_saved_state(self):
        import base64
        encoded=lambda n:base64.b64encode(json.dumps({'trust':n}).encode()).decode()
        self.assertIn('state.expedition.trust',difference({'expedition':encoded(1)},{'expedition':encoded(2)}))

    def test_process_reopen_comparison_uses_persisted_expedition_state(self):
        import base64
        def snapshot(message, accumulator):
            memory=base64.b64encode(json.dumps({
                'state':{'player':[12,34],'garden':{'revision':2}},
                'message':message,'secondsSinceSave':accumulator,
            }).encode()).decode()
            return base64.b64encode(json.dumps({
                'version':2,'expedition':memory,'camera':{'position':[12,3,34]},
            }).encode()).decode()
        expected=canonical_persistent_snapshot(snapshot('editing',4.9))
        reopened=canonical_persistent_snapshot(snapshot('reopened',0))
        self.assertIsNone(difference(expected,reopened))
        changed=copy.deepcopy(reopened);changed['expedition']['garden']['revision']=1
        self.assertIn('garden.revision',difference(expected,changed))

    def test_native_replay_waits_for_explicit_streaming_readiness(self):
        class PublishingApp:
            def __init__(self): self.calls=0
            def command(self, action):
                self.calls+=1
                settled=self.calls>=3
                return {'state':{'worldStreaming':{
                    'settled':settled,'queuedOrRunningJobs':0 if settled else 1,
                    'pendingPublication':False,'pendingUploadBatches':0,
                }}}
        app=PublishingApp()
        state=wait_for_native_readiness(app)
        self.assertEqual(app.calls,3)
        self.assertTrue(state['worldStreaming']['settled'])

    def test_failed_process_reopen_preserves_receipts_captures_and_save(self):
        import base64
        def snapshot(revision):
            memory=base64.b64encode(json.dumps({
                'state':{'garden':{'revision':revision}},'message':'','secondsSinceSave':0,
            }).encode()).decode()
            return base64.b64encode(json.dumps({
                'version':2,'expedition':memory,'camera':{'position':[0,2,0]},
            }).encode()).decode()

        class ReopenFailureApp:
            def __init__(self, directory, expected, reopened):
                self.directory=directory
                self.root=directory/'runtime'
                self.root.mkdir(parents=True)
                self.expected=expected
                self.reopened=reopened
                self.current=expected
                self.was_restarted=False
                self.pids=(41,82)
                self.saved=False
                self.events=[]

            def restart(self):
                self.events.append('restart')
                self.was_restarted=True
                return {'previousPID':self.pids[0],'currentPID':self.pids[1],
                        'previousProcessLog':str(self.directory/'app-0.log'),
                        'currentProcessLog':str(self.directory/'app-1.log'),
                        'processLog':str(self.directory/'app-1.log')}

            def command(self, action, **values):
                if action=='expeditionSlot':
                    save=self.root/'saves'/(values['name']+'.json')
                    save.parent.mkdir(parents=True,exist_ok=True)
                    save.write_text('{"persistent":true}\n')
                    if self.was_restarted:
                        self.current=self.reopened
                    return {}
                if action=='simulationRestore':
                    self.current=values['snapshot'];return {}
                if action=='saveExpedition':
                    self.saved=True
                    self.events.append('saveExpedition')
                    return {}
                if action=='simulationRun': return {}
                if action=='simulationState': return {'snapshot':self.current,'observations':{}}
                if action=='capture':
                    image=self.directory/(values['label']+'.png');image.write_bytes(b'png')
                    return {'path':str(image),'metadata':str(image.with_suffix('.json'))}
                if action=='status': return {'state':{'gpuErrors':[]}}
                raise AssertionError(action)

        with tempfile.TemporaryDirectory() as folder:
            output=Path(folder);app=ReopenFailureApp(output/'native-0',snapshot(2),snapshot(1))
            run={
                'artifact':'sanctuary-reopen.json','passed':True,
                'test':{'name':'physical-reopen'},'initial':snapshot(2),
                'frames':[
                    {'index':0,'actions':[],'state':snapshot(2),'capture':'before-reopen'},
                    {'index':1,'actions':[],'state':snapshot(2),'restore':snapshot(2),'processReopen':True},
                ],
            }
            with self.assertRaisesRegex(RuntimeError,'persisted-state mismatch'):
                render_trace(app,run)
            artifact=json.loads((output/'sanctuary-reopen.json').read_text())
            self.assertEqual([capture['label'] for capture in artifact['captures']],
                             ['before-reopen','Persisted divergence after process reopen'])
            receipt=artifact['processReopens'][0]
            self.assertEqual((receipt['previousPID'],receipt['currentPID']),(41,82))
            self.assertTrue(receipt['previousProcessLog'].endswith('app-0.log'))
            self.assertTrue(receipt['currentProcessLog'].endswith('app-1.log'))
            self.assertTrue(receipt['checkedBeforeReplayAlignment'])
            self.assertFalse(receipt['canonicalStateMatched'])
            self.assertNotEqual(receipt['expectedCanonicalSHA256'],receipt['diskCanonicalSHA256'])
            self.assertTrue(receipt['diskSaveWrittenBeforeRestart'])
            self.assertTrue(app.saved)
            self.assertLess(app.events.index('saveExpedition'),app.events.index('restart'))
            self.assertTrue((app.root/'saves/replay-physical-reopen.json').is_file())

            same=ReopenFailureApp(output/'native-1',snapshot(2),snapshot(2));same.pids=(55,55)
            same_run={
                'artifact':'sanctuary-same-pid.json','passed':True,
                'test':{'name':'same-pid'},'initial':snapshot(2),
                'frames':[{'index':0,'actions':[],'state':snapshot(2),'restore':snapshot(2),
                           'processReopen':True}],
            }
            with self.assertRaisesRegex(RuntimeError,'PID did not change'):
                render_trace(same,same_run)
            same_artifact=json.loads((output/'sanctuary-same-pid.json').read_text())
            self.assertIn('PID did not change',same_artifact['processReopens'][0]['failure'])

            class RestartFailureApp(ReopenFailureApp):
                def restart(self): raise RuntimeError('launch rejected')
            failed=RestartFailureApp(output/'native-2',snapshot(2),snapshot(2))
            failed_run={
                'artifact':'sanctuary-restart-failed.json','passed':True,
                'test':{'name':'restart-failed'},'initial':snapshot(2),
                'frames':[
                    {'index':0,'actions':[],'state':snapshot(2),'capture':'before-restart-failed'},
                    {'index':1,'actions':[],'state':snapshot(2),'restore':snapshot(2),
                     'processReopen':True},
                ],
            }
            with self.assertRaisesRegex(RuntimeError,'launch rejected'):
                render_trace(failed,failed_run)
            failed_artifact=json.loads((output/'sanctuary-restart-failed.json').read_text())
            self.assertEqual(failed_artifact['captures'][0]['label'],'before-restart-failed')
            self.assertIn('restart failed',failed_artifact['processReopens'][0]['failure'])

    def test_failure_report_is_readable_and_escapes_authored_text(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)
            report(path,dict(kind='run',passed=False,error='Expected creature',runs=[dict(name='<script>bad</script>',passed=False,failure='fear: expected <1')]))
            text=(path/'index.html').read_text()
            self.assertIn('Expected creature',text)
            self.assertIn('&lt;script&gt;',text)
            self.assertNotIn('<script>bad',text)

    def test_baselines_reject_regression_and_incompatible_workload(self):
        baseline=dict(machine={'chip':'test'},owner='cave',performanceKind='cpu',workload='one',measurements=[dict(name='one',definitionDigest='a',p95=1)])
        with tempfile.TemporaryDirectory() as folder:
            file=Path(folder)/'result.json';write_json(file,baseline)
            compare_perf(copy.deepcopy(baseline),file)
            slow=copy.deepcopy(baseline);slow['measurements'][0]['p95']=2
            with self.assertRaisesRegex(RuntimeError,'regression'):
                compare_perf(slow,file)
            changed=copy.deepcopy(baseline);changed['measurements'][0]['definitionDigest']='b'
            with self.assertRaisesRegex(RuntimeError,'definition changed'):
                compare_perf(changed,file)

    def test_performance_runs_cannot_overlap(self):
        with performance_lease():
            with self.assertRaisesRegex(RuntimeError,'Another performance'):
                with performance_lease():
                    self.fail('overlapping measurements allowed')


if __name__=='__main__':
    unittest.main()
