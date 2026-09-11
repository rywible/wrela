import argparse
from contextlib import ExitStack
import base64
from datetime import datetime
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid

from runtime import ROOT, PRODUCTS, App, gpu_lease, performance_lease, machine, source_digest, write_json
from report import report
from performance import gpu_perf, compare_visual, compare_perf

BUILT = set()


def build(product, directory):
    if product in BUILT:
        return
    command = ['swift', 'build', '-c', 'release', '--product', product] if product in ['WrelaTest','ImageCompare'] else [str(ROOT / 'scripts/build'), product]
    with (directory / ('build-' + product + '.log')).open('w') as output:
        print('Building ' + product + ' incrementally…', flush=True)
        subprocess.run(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT, check=True)
    BUILT.add(product)


def cpu(command, owner, directory, *options, workspace=ROOT):
    build('WrelaTest', directory)
    output = directory / (command + '-' + owner + '.json')
    invocation = [str(ROOT / '.build/release/WrelaTest'), command, '--game', owner, '--workspace', str(workspace), '--output', str(output), '--digest', source_digest(), *map(str, options)]
    with (directory / (command + '-' + owner + '.log')).open('w') as log:
        process = subprocess.run(invocation, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
    if process.returncode not in (0, 1) or not output.exists():
        raise RuntimeError('Runner failed; inspect ' + str(log.name))
    return json.loads(output.read_text())


def pin_content(directory):
    content = directory / 'content'
    for folder in (ROOT / 'Games').glob('*/Authoring'):
        shutil.copytree(folder, content / folder.relative_to(ROOT))
    return content


def attach_runs(directory, runs):
    for run in runs:
        name = run['owner'] + '-' + run['test']['name'] + '.json'
        run['artifact'] = name
        run['sourceDigest'] = source_digest()
        run['replayCommand'] = './scripts/test replay ' + str(directory / name)
        write_json(directory / name, run)
    return runs


def difference(expected, actual, path='state'):
    if expected == actual:
        return None
    if isinstance(expected, dict) and isinstance(actual, dict):
        for key in sorted(set(expected) | set(actual)):
            if key not in expected or key not in actual:
                return path + '.' + key + ': missing'
            diff = difference(expected[key], actual[key], path + '.' + key)
            if diff:
                return diff
    elif isinstance(expected, list) and isinstance(actual, list) and len(expected) == len(actual):
        for i, (a, b) in enumerate(zip(expected, actual)):
            diff = difference(a, b, path + f'[{i}]')
            if diff:
                return diff
    elif isinstance(expected, str) and isinstance(actual, str):
        try:
            return difference(json.loads(base64.b64decode(expected)), json.loads(base64.b64decode(actual)), path)
        except (ValueError, UnicodeError):
            pass
    return f'{path}: expected {str(expected)[:150]}, got {str(actual)[:150]}'


def render_trace(app, run, stop=None):
    app.command('simulationRestore', snapshot=run['initial'])
    captures = []
    run['captures'] = captures
    for frame in run['frames']:
        if stop is not None and frame['index'] > stop:
            break
        if frame.get('restore'):
            app.command('simulationRestore', snapshot=frame['restore'])
        actions = frame['actions']
        for i in range(0, len(actions), 500):
            app.command('simulationRun', actions=actions[i:i+500])
        state = app.command('simulationState')
        mismatch = difference(frame['state'], state['snapshot'])
        if mismatch:
            capture = app.command('capture', label='divergence')
            captures.append(dict(capture, label='First divergence at step ' + str(frame['index'])))
            run['passed']=False
            run['failure']=f'Native/headless mismatch at step {frame["index"]}: {mismatch}'
            raise RuntimeError(f'Native/headless mismatch at step {frame["index"]}: {mismatch}')
        if frame.get('capture') or frame['index'] == run.get('failureStep'):
            capture = app.command('capture', label=frame.get('capture', 'failure'))
            captures.append(dict(capture, label=frame.get('capture', 'failure')))
    if not captures:
        captures.append(dict(app.command('capture', label='final'), label='Final state'))
    state = app.command('status')['state']
    if state['gpuErrors']:
        raise RuntimeError('GPU errors: ' + str(state['gpuErrors']))
    return captures, state


def unit(directory, owners):
    if 'engine' in owners:
        with (directory/'harness-python.log').open('w') as log:
            subprocess.run([sys.executable,'-m','unittest','discover','-s',str(ROOT/'Tools/Testing'),'-p','test_*.py'],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,check=True)
    filters = {'engine':'FieldCoreTests|HarnessTests|GameContractTests', 'cave':'CaveContentTests', 'sanctuary':'SanctuaryContentTests'}
    with (directory / 'unit.log').open('w') as log:
        result = subprocess.run(['swift','test','--filter','|'.join(filters[o] for o in owners)], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError('Unit tests failed; inspect ' + str(directory / 'unit.log'))


def main():
    parser = argparse.ArgumentParser(description='One harness: CPU scenarios, seeded simulation, native rendering, authoring and performance.')
    sub = parser.add_subparsers(dest='command', required=True)
    for name in ['list','quick','run','render','dst','perf','authoring','changed','host','gpu-check']:
        p = sub.add_parser(name)
        p.add_argument('--game', choices=['all','engine','cave','sanctuary'], default='all' if name in ['list','quick','changed'] else 'sanctuary' if name=='authoring' else 'cave')
        p.add_argument('--test', default='all', help='Scenario or workload name')
        p.add_argument('--tag', default='')
        p.add_argument('--seed', type=int, default=17)
        p.add_argument('--output', type=Path)
        if name == 'dst':
            p.add_argument('--seeds', type=int, default=8)
            p.add_argument('--ticks', type=int, default=600)
        if name == 'perf':
            p.add_argument('--kind', choices=['cpu','gpu'], default='cpu')
            p.add_argument('--seconds', type=float, default=6)
            p.add_argument('--samples', type=int, default=20)
            p.add_argument('--baseline', type=Path)
        if name in ['run','render']:
            p.add_argument('--scenario',type=Path,help='An exported GameTest JSON for an agent-created reproducer')
        if name == 'render':
            p.add_argument('--baseline', type=Path)
        if name == 'authoring':
            p.add_argument('--suite', choices=['workshop','authoring','creatures','projects','review'], default='workshop')
        if name == 'changed':
            p.add_argument('--since', default='HEAD')
    for name in ['replay','inspect','minimize']:
        p = sub.add_parser(name)
        p.add_argument('artifact', type=Path)
        p.add_argument('--allow-source-change', action='store_true', help='Explicitly replay older source against current implementation')
        p.add_argument('--step', type=int)
        p.add_argument('--stage', action='store_true', help='Inspect the recorded creature brain in Soundstage')
        p.add_argument('--play', action='store_true', help='Resume the game after inspection')
        p.add_argument('--output', type=Path)
    accept = sub.add_parser('accept', help='Explicitly accept reviewed visual/performance results as a baseline')
    accept.add_argument('report', type=Path)
    accept.add_argument('--name', required=True)
    accept.add_argument('--visual-tolerance', type=float, default=0, help='Mean absolute RGB error, normalized 0..1; exact by default')
    args = parser.parse_args()
    if args.command == 'accept':
        if not args.name.replace('-','').replace('_','').isalnum():
            parser.error('Baseline name must use letters, digits, hyphens or underscores')
        if not 0 <= args.visual_tolerance <= .02:
            parser.error('Visual tolerance must be 0...0.02')
        source = args.report.resolve()
        if source.is_file():
            source = source.parent
        data = json.loads((source / 'result.json').read_text())
        if not data.get('passed') or data.get('kind') not in ['render','perf']:
            parser.error('Only a successful render or performance report can become a baseline')
        target = ROOT / 'Testing/Baselines' / args.name
        if target.exists():
            parser.error('Baseline exists; choose a new reviewed name')
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(source, target, ignore=shutil.ignore_patterns('workspace','runtime','*.log','content'))
        # Images lived below runtime; keep only referenced images with stable paths.
        for run in data.get('runs', []):
            for i, capture in enumerate(run.get('captures', [])):
                image = target / 'captures' / (run['test']['name'] + '-' + str(i) + '.png')
                image.parent.mkdir(exist_ok=True)
                shutil.copy2(capture['path'], image)
                capture['path'] = str(image.relative_to(target))
                capture.pop('comparison',None)
        data['visualTolerance'] = args.visual_tolerance
        report(target,data)
        print('Accepted baseline: ' + str(target))
        return 0
    directory = (getattr(args,'output',None) or ROOT / '.build/test-runs' / (datetime.now().strftime('%Y%m%d-%H%M%S') + '-' + args.command + '-' + uuid.uuid4().hex[:5])).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    result = dict(kind=args.command, passed=False, machine=machine(), sourceDigest=source_digest(), runs=[])
    print('Report: ' + str(directory / 'index.html'), flush=True)
    leases=ExitStack()
    try:
        if args.command=='perf':
            leases.enter_context(performance_lease())
        if args.command in ['replay','inspect','minimize']:
            artifact_path = args.artifact.resolve()
            artifact = json.loads(artifact_path.read_text())
            if artifact.get('version') != 1 or artifact.get('owner') not in ['cave','sanctuary']:
                raise RuntimeError('Unsupported artifact version/game')
            if artifact.get('sourceDigest') != source_digest() and not args.allow_source_change:
                raise RuntimeError('Source changed since this recording. Use --allow-source-change to deliberately compare the new implementation.')
            content = artifact_path.parent / 'content'
            if not content.is_dir():
                raise RuntimeError('Missing pinned content beside artifact; keep the complete run directory')
            if args.command == 'inspect':
                build('Soundstage' if args.stage else PRODUCTS[artifact['owner']], directory)
                with gpu_lease(), App('engine' if args.stage else artifact['owner'], directory / 'native', pinned=content) as app:
                    if args.stage:
                        frames=[f for f in artifact['frames'] if args.step is None or f['index']<=args.step]
                        snapshot=frames[-1]['state'] if frames else artifact['initial']
                        app.command('project',value=artifact['owner'])
                        app.command('inspectSimulation',snapshot=snapshot,label=artifact['test']['name'])
                        app.command('rig',value='softbox')
                        app.command('view',value='quarter')
                        actor=app.command('status')['state']['workshop']['document']['creature']['actor']
                        app.command('camera',orbit=.6+3.141592653589793+actor.get('yaw',0))
                        artifact['captures']=[app.command('capture',label='recorded-creature')]
                        result['native']=app.command('status')['state']
                    else:
                        artifact['captures'], result['native'] = render_trace(app, artifact, args.step)
                    artifact['artifact'] = str(artifact_path)
                    result['runs'] = [artifact]
                    result['passed'] = True
                    report(directory, result)
                    print('Native replay is open. Play or inspect it; close its window or press Ctrl-C here to finish.', flush=True)
                    if args.play:
                        app.command('pause', value=False)
                    app.process.wait()
            else:
                result['replay'] = cpu(args.command, artifact['owner'], directory, '--artifact', artifact_path, workspace=content)
                if args.command == 'minimize':
                    result['reduction']=dict(originalSteps=len(artifact['test']['steps']),reducedSteps=len(result['replay']['test']['steps']),preservedFailure=result['replay'].get('failure')==artifact.get('failure') and not result['replay']['passed'])
                    if not result['reduction']['preservedFailure']:
                        raise RuntimeError('Reduction did not preserve the failure')
                    result['runs'] = attach_runs(directory, [result['replay']])
                    shutil.copytree(content, directory / 'content')
                elif result['replay'].get('traceMatches') != 'true' or result['replay'].get('scenarioPassed') != str(artifact['passed']).lower() or result['replay'].get('failure','') != artifact.get('failure',''):
                    raise RuntimeError('Replay no longer reproduces the recorded result')
        else:
            owners = (['engine','cave','sanctuary'] if args.command in ['quick','changed','list'] else ['cave','sanctuary']) if args.game == 'all' else [args.game]
            if args.command == 'changed':
                paths = subprocess.check_output(['git','diff','--name-only',args.since],cwd=ROOT,text=True).splitlines()
                paths += subprocess.check_output(['git','ls-files','--others','--exclude-standard'],cwd=ROOT,text=True).splitlines()
                shared = any(p.startswith(('Engine/','Tools/Testing/','Tools/TestRunner/','scripts/')) or p == 'Package.swift' for p in paths)
                owners = ['engine','cave','sanctuary'] if shared else [o for o in ['cave','sanctuary'] if any(p.startswith('Games/'+PRODUCTS[o]+'/') for p in paths)]
                result['selection'] = dict(owners=owners, changedFiles=paths)
            if args.command == 'list':
                result['catalog'] = cpu('list','cave',directory)
                
                for project in result['catalog']:
                    print(project['game'] + ': ' + ', '.join(t['name'] for t in project['tests']))
            elif args.command in ['quick','changed']:
                if owners:
                    unit(directory, owners)
                    subprocess.run([str(ROOT/'scripts/check-boundaries')],cwd=ROOT,check=True)
                for owner in owners:
                    if owner != 'engine':
                        result['runs'] += cpu('run',owner,directory,'--tag','smoke','--seed',args.seed)
                pin_content(directory)
                attach_runs(directory,result['runs'])
            elif args.command in ['run','render']:
                for owner in owners:
                    if owner == 'engine':
                        raise RuntimeError('Use quick --game engine, authoring or perf --game engine for engine workloads')
                    result['runs'] += cpu('run',owner,directory,'--test',args.test,'--tag',args.tag,'--seed',args.seed,*(['--scenario',str(args.scenario.resolve())] if args.scenario else []))
                pin_content(directory)
                attach_runs(directory,result['runs'])
                if args.command == 'render':
                    for owner in owners:
                        build(PRODUCTS[owner],directory)
                    with gpu_lease():
                        for i, run in enumerate(result['runs']):
                            with App(run['owner'],directory/('native-'+str(i)),pinned=directory/'content') as app:
                                run['captures'], run['native'] = render_trace(app,run)
                                # Exercise ordinary host save/load and rejection, not just injected state.
                                saved_observations=app.command('simulationState')['observations']
                                app.command('saveExpedition')
                                app.command('expeditionSlot',name='harness-other')
                                app.command('expeditionSlot',name='expedition')
                                persisted=app.command('simulationState')['observations']
                                if persisted!=saved_observations:
                                    raise RuntimeError('Native persistence: '+run['test']['name']+' · '+difference(saved_observations,persisted))
                                try:
                                    app.command('expeditionSlot',name='../player')
                                except RuntimeError:
                                    pass
                                else:
                                    raise RuntimeError('Host accepted an invalid save path')
                                write_json(directory/run['artifact'],run)
                    if args.baseline:
                        build('ImageCompare',directory)
                        compare_visual(directory,result,args.baseline.resolve())
            elif args.command == 'dst':
                pin_content(directory)
                result['campaigns'] = []
                for owner in owners:
                    if owner == 'engine':
                        raise RuntimeError('Engine wind/weather deterministic tests are in quick --game engine; game campaigns require a game owner')
                    campaign = cpu('dst',owner,directory,'--seed',args.seed,'--seeds',args.seeds,'--ticks',args.ticks)
                    result['campaigns'].append(campaign)
                    for key in ['failure','minimized']:
                        if campaign.get(key):
                            run=campaign[key]
                            if key=='minimized':
                                run['test']['name'] += '-minimal'
                            result['runs'] += attach_runs(directory,[run])
            elif args.command == 'perf':
                if args.game == 'all':
                    raise RuntimeError('Measure one owner at a time')
                if args.kind == 'cpu':
                    result['measurements'] = cpu('bench',args.game,directory,'--test',args.test,'--samples',args.samples)
                    if not result['measurements']:
                        raise RuntimeError('No workloads matched')
                else:
                    if not 3 <= args.seconds <= 60:
                        raise RuntimeError('GPU measurements must be 3–60 seconds')
                    build(PRODUCTS[args.game],directory)
                    with gpu_lease(), App(args.game,directory/'native') as app:
                        result['measurements'] = gpu_perf(app,args,directory)
                result['owner'] = args.game
                result['performanceKind'] = args.kind
                result['workload'] = args.test
                if any(m.get('budgetBreaches') for m in result['measurements']):
                    raise RuntimeError('An explicit workload p95 budget was exceeded; inspect measurements')
                if args.baseline:
                    compare_perf(result,args.baseline.resolve())
            elif args.command == 'host':
                if args.game not in ['cave','sanctuary']:
                    raise RuntimeError('Native host checks need one game')
                build(PRODUCTS[args.game],directory)
                manifest=json.loads((ROOT/'Games'/PRODUCTS[args.game]/'Testing/NativeChecks.json').read_text())
                with gpu_lease(), App(args.game,directory/'native') as app:
                    env=dict(os.environ,WRELA_CONTROL_ROOT=str(app.root),WRELA_WORKSPACE=str(app.workspace))
                    for script in manifest['scripts']:
                        if Path(script).name!=script:
                            raise RuntimeError('Native check script must be a filename in scripts/')
                        with (directory/(script+'.log')).open('w') as log:
                            process=subprocess.run([str(ROOT/'scripts'/script)],cwd=app.workspace,env=env,stdout=log,stderr=subprocess.STDOUT)
                        if process.returncode:
                            raise RuntimeError('Native host check failed: '+script+'; inspect its log')
                    result['computerUseChecklist']=manifest['computerUse']
                    result['native']=app.command('status')['state']
                    result['runs']=[dict(name='host-'+args.game,passed=True,captures=[app.command('capture',label='native-host')])]
            elif args.command == 'gpu-check':
                if args.game not in ['cave','sanctuary']:
                    raise RuntimeError('Choose the project whose GPU fields should be verified')
                build('Soundstage',directory)
                with gpu_lease(), App('engine',directory/'native') as app:
                    app.command('project',value=args.game)
                    checks={'fields':app.command('verifyFields')['verification']}
                    if args.game=='sanctuary':
                        checks['sky']=app.command('verifySky')['verification']
                    result['verification']=checks
                    if not all(c.get('passed',False) for c in checks.values()):
                        raise RuntimeError('GPU numerical verification failed')
                    result['native']=app.command('status')['state']
            elif args.command == 'authoring':
                build('Soundstage',directory)
                with gpu_lease(), App('engine',directory/'native') as app:
                    if args.game in ['cave','sanctuary']:
                        app.command('project',value=args.game)
                    env=dict(os.environ,WRELA_CONTROL_ROOT=str(app.root),WRELA_WORKSPACE=str(app.workspace),SANCTUARY_WORKSPACE=str(app.workspace))
                    with (directory/'authoring.log').open('w') as log:
                        process=subprocess.run([str(ROOT/'scripts'/('validate-'+args.suite))],cwd=app.workspace,env=env,stdout=log,stderr=subprocess.STDOUT)
                    if process.returncode:
                        raise RuntimeError('Authoring checks failed; inspect '+str(directory/'authoring.log'))
                    result['runs']=[dict(name='authoring-'+args.suite,passed=True,captures=[app.command('capture',label='authoring-result')])]
                    result['native']=app.command('status')['state']
        result['passed'] = args.command in ['inspect','minimize'] or (all(run.get('passed',False) for run in result['runs']) and all(c['passed'] for c in result.get('campaigns',[])))
    except KeyboardInterrupt:
        result['error']='Interrupted; owned app cleaned up'
    except Exception as error:
        result['error']=str(error)
        print(str(error),file=sys.stderr)
    leases.close()
    result['endingSourceDigest']=source_digest()
    if result['endingSourceDigest']!=result['sourceDigest']:
        result['passed']=False
        result['error']='Source changed during the run; rerun against a stable checkout'
    report(directory,result)
    for run in result['runs']:
        print(('PASS' if run['passed'] else 'FAIL') + ' ' + run.get('test',{}).get('name',run.get('name','')) + (' · '+run['failure'] if run.get('failure') else ''),flush=True)
    print(('PASS' if result['passed'] else 'FAIL')+' · '+str(directory/'index.html'),flush=True)
    return 0 if result['passed'] else 1

