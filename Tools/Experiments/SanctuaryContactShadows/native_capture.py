#!/usr/bin/env python3
"""Root-only native fixture capture. Requires an already-running isolated app.

This sends real production harness commands. It never launches/builds an app or
uses the default control root. Run serially, outside every other GPU workload.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import runpy
import sys
import time


def state_errors(state, expected, sky):
    """Check actual capture metadata, not only the earlier status acknowledgement."""
    failures=[]
    def close(actual, wanted, path):
        if isinstance(wanted, dict):
            if not isinstance(actual, dict):
                failures.append(path+' is missing');return
            for key,value in wanted.items(): close(actual.get(key),value,path+'.'+key)
        elif isinstance(wanted, list):
            if not isinstance(actual, list) or len(actual)!=len(wanted):
                failures.append(path+' has the wrong shape');return
            for index,value in enumerate(wanted): close(actual[index],value,f'{path}[{index}]')
        elif isinstance(wanted, (int,float)) and not isinstance(wanted,bool):
            if (not isinstance(actual,(int,float)) or isinstance(actual,bool)
                or not math.isfinite(actual) or not math.isfinite(wanted) or abs(actual-wanted)>0.0001):
                failures.append(path+' differs from the requested fixture')
        elif actual!=wanted:
            failures.append(path+' differs from the requested fixture')
    close(state.get('camera'),expected['camera'],'camera')
    close(state.get('sky'),sky,'sky')
    close(state.get('lighting'),expected['lighting'],'lighting')
    close(state.get('wetness'),expected['wetness'],'wetness')
    close(state.get('parameters',{}).get('exposure'),expected['parameters']['exposure'],'exposure')
    close(state.get('sceneLook'),expected['sceneLook'],'sceneLook')
    if 'outdoorAmbientFloor' in expected:
        close(state.get('outdoorAmbientFloor'),expected['outdoorAmbientFloor'],'outdoorAmbientFloor')
    cache=state.get('skyCache',{})
    for key in ['requestedSunDiffersFromCache','sceneSunLeadsCache','pendingAuthoring']:
        if cache.get(key) is not False: failures.append('skyCache.'+key+' is not settled/known')
    for key in ['sourceDigest','shaderDigest']:
        if not isinstance(state.get(key),str) or len(state[key])!=64:
            failures.append(key+' is unavailable')
    return failures


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--control-root',type=Path,required=True)
    parser.add_argument('--receipt',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--label',required=True)
    parser.add_argument('--sun-altitude',type=float)
    args=parser.parse_args()
    control=args.control_root.resolve()
    if not (control/'status.json').is_file(): parser.error('The explicit control root has no live status.')
    receipt_bytes=args.receipt.read_bytes();receipt=json.loads(receipt_bytes)
    snapshot=receipt['simulation']['snapshot'];expected=receipt['status']['state']
    os.environ['WRELA_CONTROL_ROOT']=str(control)
    command=runpy.run_path(str(Path(__file__).resolve().parents[2]/'AgentTools/control.py'))['command']
    # simulationRestore rejects ordinary, non-testing hosts before any restore.
    command('simulationRestore',snapshot=snapshot)
    command('pause',value=True)
    command('lighting',value=expected['lighting'])
    sky={k:v for k,v in expected['sky'].items() if k in
         ['altitude','azimuth','coverage','density','haze','cloudSeed']}
    if args.sun_altitude is not None: sky['altitude']=args.sun_altitude
    command('sky',**sky)
    command('parameters',exposure=expected['parameters']['exposure'],wetness=expected['wetness'])
    command('look',**expected['sceneLook'])
    started=time.monotonic()
    while True:
        state=command('status')['state'];stream=state.get('worldStreaming',{})
        if stream.get('settled') and not stream.get('pendingPublication') and not stream.get('pendingUploadBatches'):
            break
        if time.monotonic()-started>60: raise RuntimeError('Publication did not settle; no comparison captures made.')
        time.sleep(.25)
    # A mismatched restored camera invalidates a pixel comparison. Do not correct
    # it by teleporting the player or changing production collision rules.
    for key in ['yaw','pitch']:
        if abs(state['camera'][key]-expected['camera'][key])>1e-5:
            raise RuntimeError(f'Restored camera {key} differs from receipt.')
    if max(abs(a-b) for a,b in zip(state['camera']['position'],expected['camera']['position']))>1e-4:
        raise RuntimeError('Restored camera position differs from receipt.')
    if any(state.get(key,{}).get('active') for key in ['naturePreview','placementPreview']):
        raise RuntimeError('Cancel transient tools in the isolated app before comparison; no captures made.')
    captures=[command('capture',label=args.label+suffix) for suffix in ['-first','-repeat']]
    png_hashes=[hashlib.sha256(Path(c['path']).read_bytes()).hexdigest() for c in captures]
    metadata=[json.loads(Path(c['metadata']).read_text()) for c in captures]
    errors=[{'capture':index,'errors':state_errors(value,expected,sky)}
            for index,value in enumerate(metadata)]
    repeated=len(set(png_hashes))==1
    stable_fingerprints=all(metadata[0].get(key)==metadata[1].get(key)
                            for key in ['sourceDigest','shaderDigest'])
    eligible=repeated and stable_fingerprints and not any(item['errors'] for item in errors)
    result={'inputReceipt':str(args.receipt.resolve()),'inputReceiptSHA256':hashlib.sha256(receipt_bytes).hexdigest(),
        'restoredSnapshotSHA256':hashlib.sha256(snapshot.encode()).hexdigest(),'sky':sky,
        'status':command('status'),'captures':captures,'controlRoot':str(control),
        'pngSHA256':png_hashes,'exactPNGBytesRepeated':repeated,
        'captureMetadataSHA256':[hashlib.sha256(Path(c['metadata']).read_bytes()).hexdigest() for c in captures],
        'captureValidation':errors,'stableCaptureFingerprints':stable_fingerprints,
        'comparisonEligible':eligible,
        'transientTools':'inactive for every new reference and variant; original receipt may show tools',
        'rendererTime':'simulationRestore resets renderer clock and wind; saved production brain is restored'}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'report':str(args.output),'captures':captures},indent=2))
    if not eligible:
        print('Capture inputs, fingerprints or exact repeat differed; inspect the written report. '
              'This run is not eligible for A/B acceptance.',file=sys.stderr)
        raise SystemExit(1)


if __name__=='__main__':main()
