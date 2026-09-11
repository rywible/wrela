"""Game/engine workload measurement and explicit baseline comparisons."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time
from runtime import ROOT, PRODUCTS
from report import images_compare

def gpu_perf(app,args,directory):
    manifest=ROOT/('Engine/Testing/RenderWorkloads.json' if args.game=='engine' else 'Games/'+PRODUCTS[args.game]+'/Testing/RenderWorkloads.json')
    workloads=json.loads(manifest.read_text())
    selected=[w for w in workloads if args.test=='all' or w['name']==args.test]
    if not selected:
        raise RuntimeError('Unknown GPU workload; see '+str(manifest))
    measurements=[]
    for workload in selected:
        for command in workload['setup']:
            command=dict(command);app.command(command.pop('action'),**command)
        app.command('profiling',value=True)
        app.command('pause',value=False)
        time.sleep(3)
        app.command('clearMetrics')
        begin=app.command('status')['state']
        started=time.monotonic();next_camera=started
        while time.monotonic()-started<args.seconds:
            if workload.get('moving') and time.monotonic()>=next_camera:
                state=app.command('status')['state']
                if args.game=='engine':
                    app.command('camera',orbit=state['workshop']['document']['orbit']+.025)
                else:
                    app.command('camera',yaw=state['camera']['yaw']+.025)
                next_camera=time.monotonic()+.25
            time.sleep(.05)
        end=app.command('status')['state']
        end['residentBytes']=int(subprocess.check_output(['ps','-o','rss=','-p',str(app.process.pid)],text=True).strip())*1024
        app.command('pause',value=True)
        if end['gpuErrors'] or end['timingSampleCount']<60:
            raise RuntimeError('GPU workload failed or did not produce enough samples')
        if end['thermalState']>=2 or end['lowPowerMode']:
            raise RuntimeError('Performance environment is thermally constrained or in low power mode; retry after cooling')
        budgets=workload.get('budgets',{})
        breaches=[key for key,limit in budgets.items() if end[key]['p95']>limit]
        measurements.append(dict(budgetBreaches=breaches,name=workload['name'],definitionDigest=hashlib.sha256(json.dumps(workload,sort_keys=True).encode()).hexdigest(),seconds=time.monotonic()-started,begin=begin,end=end))
    return measurements


def baseline_file(path):
    return path/'result.json' if path.is_dir() else path


def compare_visual(directory,result,path):
    baseline=json.loads(baseline_file(path).read_text());base_dir=baseline_file(path).parent
    if baseline['machine']!=result['machine']:
        raise RuntimeError('Visual baseline belongs to a different machine/OS')
    for run in result['runs']:
        previous=next((r for r in baseline['runs'] if r['test']==run['test'] and r['owner']==run['owner'] and r['seed']==run['seed']),None)
        if previous is None or len(previous.get('captures',[]))!=len(run.get('captures',[])):
            raise RuntimeError('Visual baseline does not match scenario/seed/captures')
        for i,(actual,expected) in enumerate(zip(run['captures'],previous['captures'])):
            expected_copy=directory/(run['test']['name']+f'-{i}-expected.png')
            shutil.copy2(base_dir/expected['path'],expected_copy)
            metrics=images_compare(actual['path'],expected_copy,directory/(run['test']['name']+f'-{i}-diff.png'))
            metrics['expected']=str(expected_copy)
            if metrics.get('meanAbsoluteError',1)>baseline.get('visualTolerance',0):
                run['passed']=False;run['failure']='Visual baseline differs: '+json.dumps(metrics)
            actual['comparison']=metrics


def compare_perf(result,path):
    baseline=json.loads(baseline_file(path).read_text())
    for key in ['machine','owner','performanceKind','workload']:
        if result.get(key)!=baseline.get(key):
            raise RuntimeError('Performance baseline mismatch: '+key)
    regressions=[]
    for current in result['measurements']:
        old=next((v for v in baseline['measurements'] if v['name']==current['name']),None)
        if old is None:
            raise RuntimeError('Missing baseline workload '+current['name'])
        if 'end' in current:
            if current['definitionDigest']!=old['definitionDigest'] or current['end']['renderSize']!=old['end']['renderSize']:
                raise RuntimeError('GPU workload definition/resolution changed')
            values=[(k,current['end'][k]['p95'],old['end'][k]['p95']) for k in ['gpuMilliseconds','cpuSimulationMilliseconds','frameIntervalMilliseconds']]
        else:
            if current['definitionDigest']!=old['definitionDigest']:
                raise RuntimeError('CPU workload definition changed')
            values=[('p95',current['p95'],old['p95'])]
        for metric,value,reference in values:
            if value>reference*1.20 and value-reference>.2:
                regressions.append(dict(workload=current['name'],metric=metric,baseline=reference,actual=value))
    result['regressions']=regressions
    if regressions:
        raise RuntimeError('Performance regression exceeds 20% and 0.2 ms; inspect measurements')
