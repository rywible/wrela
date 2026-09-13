#!/usr/bin/env python3
"""Root-operated, hash-checked application/reversal of one shadow diagnostic.

Prepare a disposable source copy; never point this at the preserved candidate.
This program launches no app and builds nothing.
"""
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import tempfile


def digest(data): return hashlib.sha256(data).hexdigest()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode',choices=['check','apply','revert'])
    parser.add_argument('variant',choices=['A','B'])
    parser.add_argument('--root',type=Path,required=True)
    args=parser.parse_args()
    root=args.root.resolve();pack=Path(__file__).resolve().parent/'patches'
    manifest=json.loads((pack/'manifest.json').read_text())
    spec=manifest['variants'][args.variant]
    contents={}
    for path,expected in manifest['baseFiles'].items():
        target=root/path
        if target.is_symlink() or not target.is_file():
            parser.error(f'Missing or symlinked diagnostic source: {target}')
        contents[path]=target.read_bytes()
        allowed={expected,spec['afterSHA256']} if path==spec['path'] else {expected}
        if digest(contents[path]) not in allowed:
            parser.error(f'Source hash mismatch: {path}. No writes made; do not combine variants or silently rebase.')
    patch=(pack/f'{args.variant}.patch').read_bytes()
    if digest(patch)!=spec['patchSHA256']: parser.error('Patch fingerprint mismatch; no writes made.')
    current=contents[spec['path']]
    state='reference' if digest(current)==spec['beforeSHA256'] else args.variant
    old,new=spec['beforeChunk'].encode(),spec['afterChunk'].encode()
    before=current if state=='reference' else current.replace(new,old,1)
    if before.count(old)!=1: parser.error('Replacement is not unique; no writes made.')
    after=before.replace(old,new,1)
    expected_patch=''.join(difflib.unified_diff(before.decode().splitlines(True),after.decode().splitlines(True),
        fromfile='a/'+spec['path'],tofile='b/'+spec['path'])).encode()
    if (digest(before)!=spec['beforeSHA256'] or digest(after)!=spec['afterSHA256']
            or expected_patch!=patch): parser.error('Patch/payload mismatch; no writes made.')
    if args.mode!='check':
        desired=after if args.mode=='apply' else before
        target=root/spec['path']
        if desired!=current:
            if target.read_bytes()!=current: parser.error('Source changed during verification; no writes made.')
            permissions=target.stat().st_mode & 0o777
            with tempfile.NamedTemporaryFile(dir=target.parent,prefix='.shadow-diagnostic-',delete=False) as f:
                temporary=Path(f.name);f.write(desired);f.flush();os.fsync(f.fileno())
            try:
                temporary.chmod(permissions)
                if target.read_bytes()!=current: parser.error('Source changed before replacement; no writes made.')
                os.replace(temporary,target)
            finally:
                temporary.unlink(missing_ok=True)
        state=args.variant if args.mode=='apply' else 'reference'
    print(json.dumps({'root':str(root),'mode':args.mode,'state':state,'variant':args.variant,
        'requiresBuildAndReopen':spec['rebuildRequired'],
        'sourceSHA256':{p:digest((root/p).read_bytes()) for p in manifest['baseFiles']},
        'patchSHA256':spec['patchSHA256']},indent=2))


if __name__=='__main__': main()
