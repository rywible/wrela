#!/usr/bin/env python3
"""Vesper production anatomy, authored through the shared native field transaction."""
import sys,math,json
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'Tools/AgentTools'))
from creature import CreatureWorkspace
COAT=[.255,.215,.163]; WARM=[.315,.265,.191]; PAW=[.35,.292,.215]; PAD=[.064,.050,.035]
def E(key,region,center,radius,weights,*,color=COAT,blend=.025,rotation=(0,0,0),end=(0,0,0),primitive='ellipsoid',operation='add',role='surface'):
 return dict(id=key,region=region,center=list(center),radius=list(radius),jointWeights=weights,color=color,blend=blend,rotation=list(rotation),end=list(end),primitive=primitive,operation=operation,role=role)
def source(part,elements,resolution=64):
 return dict(id='vesper-'+part,part=part,elements=elements,resolution=resolution,material=9,roughness=.86,metallic=0)
def along(key,region,a,b,width,depth,weights,**kw):
 d=[b[i]-a[i] for i in range(3)];l=math.sqrt(sum(x*x for x in d))
 rot=[math.degrees(math.asin(d[2]/l)),0,math.degrees(math.atan2(-d[0],d[1]))]
 return E(key,region,[(a[i]+b[i])/2 for i in range(3)],[width,l*.55,depth],weights,rotation=rot,**kw)
def add(a,b):return [a[i]+b[i] for i in range(3)]
def lerp(a,b,t):return [a[i]+(b[i]-a[i])*t for i in range(3)]
def sources():
 body=[E('ribcage','ribs',[0,1.96,-.27],[.70,.77,.99],{'spine-1':1},blend=.04),
 E('loin','abdomen',[0,1.69,.77],[.58,.59,.81],{'body':.65,'haunch':.35},blend=.14),
 E('pelvis','pelvis',[0,1.60,1.49],[.68,.66,.62],{'haunch':1},blend=.10),
 E('sternum','chest',[0,1.81,-.84],[.43,.42,.39],{'spine-2':1},blend=.10),
 along('neck-column','neck',[0,2.13,-.84],[0,2.95,-1.59],.51,.43,{'neck':1},blend=.13),
 E('occiput','neck',[0,2.98,-1.46],[.43,.42,.40],{'neck':.5,'mask':.5},blend=.07)]
 for s in (-1,1):
  side='left' if s<0 else 'right';fore='fore-'+side;hind='hind-'+side
  body += [E('scapula-'+side,'shoulder',[s*.50,2.18,-.54],[.31,.49,.49],{'neck':.6,fore+'-upper':.4},rotation=[-22,0,s*-12],blend=.14,color=WARM),
   E('pectoral-'+side,'chest',[s*.30,1.93,-.92],[.30,.32,.25],{'neck':.75,fore+'-upper':.25},rotation=[-10,0,s*12],blend=.11),
   E('hip-'+side,'hip',[s*.51,1.62,1.50],[.33,.47,.44],{'haunch':.7,hind+'-upper':.3},rotation=[15,0,s*10],blend=.13),
   along('neck-tendon-'+side,'neck',[s*.31,2.13,-1.10],[s*.23,2.86,-1.59],.12,.16,{'neck':1},blend=.04,color=WARM),
   E('scapula-bone-'+side,'scapula',[s*.59,2.19,-.53],[.075,.42,.31],{'neck':1},role='internalStructure')]
 yield source('body',body,96)
 for front in (True,False):
  for s in (-1,1):
   side='left' if s<0 else 'right';key=('fore-' if front else 'hind-')+side
   a,b,c=([s*.64,2.25,-.85],[s*.82,1.18,.05],[s*.93,.18,-1.3]) if front else ([s*.58,1.5,1.55],[s*.82,.84,2.2],[s*.87,.18,1.45])
   upper=key+'-upper';lower=key+'-lower';foot=key+'-paw'
   leg=[E('upper-bone','bone',a,[.065]*3,{upper:1},end=b,primitive='capsule',role='internalStructure'),
    E('humerus-core','upper-limb',a,[.195 if front else .22]*3,{upper:1},end=b,primitive='capsule',blend=.02),
    along('extensor','upper-limb',lerp(a,b,.03),lerp(a,b,.76),.32 if front else .36,.27 if front else .31,{upper:1},blend=.05,color=WARM),
    along('flexor','upper-limb',add(lerp(a,b,.13),[0,0,-.12]),add(lerp(a,b,.82),[0,0,-.09]),.245,.215,{upper:1},blend=.045),
    E('elbow','joint',b,[.225,.225,.215],{upper:.5,lower:.5},blend=.05),
    along('lower-core','lower-limb',b,c,.175,.175,{lower:1},blend=.04,color=WARM),
    along('forearm-extensor','lower-limb',lerp(b,c,.04),lerp(b,c,.65),.195,.20,{lower:1},blend=.035),
    E('carpal','wrist',add(c,[0,.105,.005]),[.21,.205,.19],{lower:.5,foot:.5},blend=.035,color=PAW)]
   for k in (-1,1):
    leg.append(along('tendon-'+str(k),'tendon',add(lerp(b,c,.35),[k*.09,0,-.07]),add(lerp(b,c,.92),[k*.07,0,-.05]),.037,.056,{lower:1},blend=.012,color=WARM))
   yield source(upper,leg,64)
   # Retained lower part identity supplies a subtle patellar surface instead of a gold sphere.
   yield source(lower,[E('olecranon','joint',add(b,[0,.015,.04]),[.16,.155,.165],{lower:1},color=COAT)],48)
   paw=[E('metacarpus','paw',add(c,[0,-.005,-.11]),[.305,.17,.335],{foot:1},color=PAW,blend=.025),
    E('heel-pad','pad',add(c,[0,-.10,-.03]),[.215,.070,.21],{foot:1},color=PAD,blend=.008)]
   for i,x in enumerate((-.245,-.084,.084,.245)):
    reach=(.29,.37,.385,.305)[i]
    toe=add(c,[x,-.033,-reach])
    paw.append(E('digit-'+str(i),'toe',toe,[.114,.137,.205],{foot:1},rotation=[0,(i-1.5)*-6,0],color=PAW,blend=.019))
    paw.append(E('pad-'+str(i),'pad',add(toe,[0,-.085,-.025]),[.080,.048,.113],{foot:1},color=PAD,blend=.005))
    paw.append(E('claw-'+str(i),'claw',add(toe,[0,-.048,-.178]),[.039,.051,.12],{foot:1},rotation=[-12,0,0],color=[.105,.081,.052],blend=.004))
   yield source(foot,paw,64)
def connected_sources():
 values=list(sources()); body=values[0]; replacing=[]
 for item in values[1:]:
  if item['part'].endswith(('-upper','-lower')):
   replacing.append(item['part'])
   for element in item['elements']:
    element['id']=item['part']+'-'+element['id']
    if element['operation']=='add' and element['role']=='surface':
     element['blend']=max(element['blend'],.075)
    body['elements'].append(element)
 body['resolution']=128;body['replaces']=replacing;body['skinBlend']=.12
 return [body]+[item for item in values[1:] if item['part'].endswith('-paw')]

def main():
 s=CreatureWorkspace()
 # Retire earlier local fields that were fitted to the round prototype mask.
 a=s.command('authoring')['authoring']
 # The revision comes from native source; arrays replace atomically.
 old=a.get('source',{})
 revision=a.get('revision')
 if revision:
  s.command('author',expectedRevision=revision,surfaceEdits=[e for e in old.get('surfaceEdits',[]) if e['part'] not in ('mask','eyes')])
 with s.edit() as e:
  for old_source in e.snapshot['source'].get('anatomy',[]):
   if old_source['part'].endswith(('-upper','-lower')):e.remove('anatomy',old_source['id'])
  for v in connected_sources():e.upsert('anatomy',v)
  for stroke in e.snapshot['source'].get('strokes',[]):
   if stroke['part']=='mask':e.remove('strokes',stroke['id'])
  # These fields were fitted to the old ellipsoid body. The connected source
  # now owns regional bindings; stacking the old fields destroys that intent.
  for field in e.snapshot['source'].get('skinFields',[]):
   if field['id'] in ('spine-root','spine-chest'):e.remove('skinFields',field['id'])
  # Field extraction establishes the useful resolution directly.
  e.set('refinement',{})
 print(json.dumps({'authoredAnatomy':[x['part'] for x in connected_sources()]}))
if __name__=='__main__':main()
