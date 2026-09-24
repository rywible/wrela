"""Preserve the measured follow-up, screenshots and motion comparison."""
from pathlib import Path
import json
import statistics
import shutil
from PIL import Image,ImageDraw
ROOT=Path('docs/research');stem='compiler-frontiers-rendered-2026-09-21'
material=json.loads(Path('output/compiler-frontiers/material-latest.json').read_text())
branch=json.loads(Path('output/compiler-frontiers/branch-spatial/result.json').read_text())
gpu=json.loads(Path('output/compiler-frontiers/branch-spatial/gpu.json').read_text())
for result in [material,gpu]:
    manifest=json.loads((Path(result['output'])/'run-manifest.json').read_text())
    assert manifest['sourceStable'];result['runManifest']=manifest
summary=[]
for t in material['timings']:
    values=sorted(r['gpuMs'] for r in t['result']['timing'])
    summary.append(dict(trial=t['trial'],mode=t['mode'],samples=len(values),medianMs=statistics.median(values),p95Ms=values[int(.95*len(values))]))
result=dict(material=material,materialTimingSummary=summary,branch=branch,branchGpu=gpu)
winter=Path("output/compiler-frontiers/winter-control.json")
if winter.exists():result["winterControl"]=json.loads(winter.read_text())
(ROOT/(stem+'.json')).write_text(json.dumps(result,indent=2))
source=Path(material['output'])
def pair(a,b):
    images=[Image.open(a).convert('RGB'),Image.open(b).convert('RGB')]
    w,h=images[0].size;canvas=Image.new('RGB',(2*w,h+30),'#101923');draw=ImageDraw.Draw(canvas)
    for i,(im,label) in enumerate(zip(images,['Point sample','Compiler integrated'])):
        canvas.paste(im,(i*w,30));draw.text((i*w+12,9),label,fill='white')
    return canvas
pair(source/'weave-point.png',source/'weave-compiled.png').save(ROOT/(stem+'.weave.png'))
frames=[pair(source/f'motion-point-{i}.png',source/f'motion-compiled-{i}.png') for i in range(8,16)]
frames[0].save(ROOT/(stem+'.weave.gif'),save_all=True,append_images=frames[1:],duration=160,loop=0)
for name,ext in [('comparison.png','branch.png'),('motion.gif','branch.gif')]:
    shutil.copyfile(Path('output/compiler-frontiers/branch-spatial')/name,ROOT/(stem+'.'+ext))
print(json.dumps(summary,indent=2))
