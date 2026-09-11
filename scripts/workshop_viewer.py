import json,os,hashlib
from pathlib import Path

VIEWER=Path(__file__).with_name("workshop-review.html").read_text()

def write_viewer(folder,records,art=None):
    items=[]
    for r in records:
        m=json.loads(Path(r['metadata']).read_text())
        items.append(dict(variant=r['variant'],condition=r['condition'],view=r['view'],frame=r['frame'],src=os.path.relpath(r['path'],folder),time=m['time'],creature=m.get('workshop',{}).get('document',{}).get('creature'),shader=m['shaderDigest'],source=m.get('sourceDigest'),imageSHA256=hashlib.sha256(Path(r['path']).read_bytes()).hexdigest(),subject=m['studioObject'],height=m['workshop']['heightMetres'],gpu=m['gpuMilliseconds'].get('median'),document=m['workshop']['document']))
    text=VIEWER.replace('__ITEMS__',json.dumps(items).replace('<','\\u003c'))
    text=text.replace('__ART__',json.dumps(art).replace('<','\\u003c'))
    (folder/'index.html').write_text(text)
