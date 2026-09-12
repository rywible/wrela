#!/usr/bin/env python3
"""Lion proportion rebuild, using public anatomy, rig and contact interfaces.

Artist reference/critique: docs/studies/VESPER_LION_REFERENCE.md.
Preserve the incoming study before applying. This changes studio source only;
old finery is hidden until its attachments are reauthored to the new anatomy.
"""
import argparse, copy, json, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'Tools/AgentTools'))
from creature import CreatureWorkspace
from author_vesper_anatomy import E, along, add, lerp, source
COAT=[.33,.28,.21]

def joint(key,pivot,parent=None):
 return dict(id=key,pivot=pivot,**({} if parent is None else dict(parent=parent)))

def legs():
 for front in (True,False):
  for s in (-1,1):
   key=('fore' if front else 'hind')+('-left' if s<0 else '-right')
   if front:
    a,b,h,c=[s*.48,1.91,-1.02],[s*.51,1.12,-.91],[s*.52,.24,-1.08],[s*.52,.16,-1.10]
   else:
    a,b,h,c=[s*.47,1.92,1.43],[s*.57,1.12,1.06],[s*.58,.47,1.83],[s*.58,.16,1.61]
   yield key,front,s,a,b,h,c

def rig():
 j=[joint('body',[0,1.9,.55]),joint('haunch',[0,1.94,1.40],'body'),
    joint('spine-0',[0,2.13,.43],'body'),joint('spine-1',[0,2.18,-.12],'spine-0'),
    joint('spine-2',[0,2.22,-.68],'spine-1'),joint('neck',[0,2.24,-1.12],'spine-2'),
    joint('mask',[0,2.45,-1.91],'neck'),joint('jaw',[0,2.20,-2.03],'mask'),
    joint('eyes',[0,2.52,-2.29],'mask'),joint('tail',[0,2.02,1.97],'haunch')]
 for key,front,s,a,b,h,c in legs():
  j += [joint(key+'-upper',a,'spine-2' if front else 'haunch'),
        joint(key+'-lower',b,key+'-upper'),joint(key+'-paw',h,key+'-lower')]
 return j

def anatomy():
 b=[E('ribcage','ribs',[0,1.81,-.14],[.58,.64,1.18],{'spine-1':1},blend=.10),
    E('loin','abdomen',[0,1.92,.84],[.455,.48,.85],{'body':.65,'haunch':.35},blend=.24),
    E('pelvis','pelvis',[0,1.98,1.45],[.50,.43,.59],{'haunch':1},blend=.18),
    E('sternum','chest',[0,1.60,-.83],[.37,.43,.43],{'spine-2':1},blend=.18),
    along('neck-column','neck',[0,2.07,-.86],[0,2.44,-1.78],.405,.43,{'neck':.75,'spine-2':.25},blend=.21),
    along('nuchal-mass','neck',[0,2.37,-.72],[0,2.63,-1.67],.29,.245,{'neck':.55,'spine-2':.45},blend=.17),
    E('occiput','neck',[0,2.47,-1.77],[.345,.345,.35],{'neck':.45,'mask':.55},blend=.13)]
 for key,front,s,a,k,h,c in legs():
  side='left' if s<0 else 'right';u=key+'-upper';l=key+'-lower';f=key+'-paw'
  if front:
   b += [along('scapula-'+side,'shoulder',[s*.43,2.39,-.43],a,.165,.34,{'spine-2':.8,u:.2},blend=.22),
         along('triceps-'+side,'upper-limb',add(a,[0,.02,.08]),add(k,[0,.10,.09]),.190,.245,{u:1},blend=.15),
         along('pectoral-'+side,'chest',[s*.20,1.66,-1.00],add(a,[0,-.15,-.03]),.20,.20,{'spine-2':.8,u:.2},blend=.12)]
  else:
   b += [along('gluteal-'+side,'hip',add(a,[0,.17,.13]),lerp(a,k,.6),.215,.34,{'haunch':.4,u:.6},blend=.20),
         along('quadriceps-'+side,'thigh',a,add(k,[0,.10,-.03]),.205,.29,{u:1},blend=.16)]
  b += [E(key+'-bone','internal-limb',a,[.055]*3,{u:1},primitive='capsule',end=k,role='internalStructure'),
        E(key+'-upper-core','upper-limb',a,[.145]*3,{u:1},primitive='capsule',end=k,blend=.065),
        E(key+'-joint','elbow' if front else 'stifle',k,[.155,.18,.18],{u:.5,l:.5},blend=.085),
        along(key+'-lower-core','forearm' if front else 'shin',k,h,.133,.155,{l:1},blend=.065),
        along(key+'-lower-muscle','forearm' if front else 'calf',lerp(k,h,.06),lerp(k,h,.61),.157,.18,{l:1},blend=.06),
        E(key+'-wrist','carpus' if front else 'hock',h,[.13,.15,.14],{l:.5,f:.5},blend=.055)]
 result=source('body',b,128)
 result['replaces']=[key+suffix for key,*_ in legs() for suffix in ('-upper','-lower')]
 result['skinBlend']=.14
 yield result
 for key,front,s,a,k,h,c in legs():
  f=key+'-paw'
  paw=[along('metapodial','metacarpus' if front else 'metatarsus',h,add(c,[0,0,-.05]),.127,.14,{f:1},blend=.06),
       E('palm','paw',add(c,[0,-.015,-.09]),[.235,.135,.25],{f:1},blend=.055)]
  for i,x in enumerate((-.172,-.06,.06,.172)):
   reach=(.23,.275,.29,.245)[i]
   paw.append(E('digit-'+str(i),'digit',add(c,[x,-.035,-reach]),[.087,.106,.14],{f:1},blend=.033))
  yield source(f,paw,80)
 # A living feline head: closed lips, small occupied eyes, rounded ears.
 # Each major volume has a semantic source identity for subsequent refinement.
 w={'mask':1}
 head=[E('cranium','cranium',[0,2.485,-1.97],[.40,.34,.46],w,blend=.06),
       E('frontal','forehead',[0,2.64,-2.15],[.30,.15,.32],w,blend=.16),
       along('nasal-bridge','nasal',[0,2.58,-2.27],[0,2.35,-2.68],.178,.165,w,blend=.10),
       E('nasal-tip','nose',[0,2.345,-2.737],[.158,.065,.075],w,blend=.035)]
 for s in (-1,1):
  side='left' if s<0 else 'right'
  head += [E('masseter-'+side,'cheek',[s*.28,2.36,-1.96],[.145,.235,.285],w,rotation=[0,0,s*-10],blend=.18),
           along('zygomatic-'+side,'cheekbone',[s*.32,2.48,-1.98],[s*.28,2.49,-2.37],.085,.13,w,blend=.11),
           E('whisker-pad-'+side,'muzzle',[s*.134,2.223,-2.605],[.153,.11,.184],w,rotation=[0,s*7,0],blend=.065),
           E('brow-'+side,'brow',[s*.254,2.606,-2.328],[.153,.044,.134],w,rotation=[0,s*-12,s*9],blend=.07),
           E('ear-'+side,'ear',[s*.34,2.778,-1.825],[.137,.170,.082],w,rotation=[-10,s*-20,s*-20],blend=.055)]
 for s in (-1,1):
  side='left' if s<0 else 'right'
  head += [E('orbit-'+side,'orbit',[s*.292,2.535,-2.360],[.105,.066,.108],{},rotation=[0,s*-20,s*8],operation='cut',blend=0),
           E('ear-concha-'+side,'ear',[s*.344,2.798,-1.884],[.087,.106,.060],{},rotation=[-10,s*-20,s*-20],operation='cut',blend=0),
           E('nostril-'+side,'nostril',[s*.108,2.341,-2.791],[.031,.020,.032],{},operation='cut',blend=0)]
 head.append(E('philtrum','muzzle',[0,2.18,-2.772],[.010,.060,.032],{},operation='cut',blend=0))
 yield source('mask',head,128)
 jaw=[E('chin','chin',[0,2.091,-2.535],[.215,.075,.216],{'jaw':1},blend=.04)]
 for s in (-1,1):
  jaw += [along('ramus-'+str(s),'mandible',[s*.26,2.23,-2.02],[s*.17,2.10,-2.51],.092,.104,{'jaw':1},blend=.065)]
 yield source('jaw',jaw,96)
 eyes=[]
 for s in (-1,1):
  eyes += [E('globe-'+str(s),'eye',[s*.291,2.535,-2.333],[.084,.058,.078],{'eyes':1},blend=0,color=[.31,.23,.08]),
           E('pupil-'+str(s),'pupil',[s*.302,2.535,-2.407],[.024,.028,.013],{'eyes':1},blend=0,color=[.013,.012,.009])]
 eye=source('eyes',eyes,96);eye['roughness']=.26
 yield eye

def apply():
 s=CreatureWorkspace(); snapshot=s.command('authoring')['authoring']; old=snapshot['source']; c=copy.deepcopy(old['craft'])
 new=list(anatomy()); changed={x['part'] for x in new}
 c['anatomy']=new
 c['joints']=rig()
 for name in ('strokes','skinFields','correctives','landmarks'):
  c[name]=[x for x in c.get(name,[]) if x.get('part',x.get('field',{}).get('part')) not in changed]
 # No incoming anatomical anchors exist for these replaced sources. Explicitly
 # refuse a destructive reset if a future source has acquired correspondence.
 assert not c.get('anatomyAnchors'), 'Rebind existing anatomical anchors before replacing source.'
 c['contactChains']=[]
 for key,front,sign,a,k,h,p in legs():
  c['contactChains'].append(dict(id=key,parent='spine-2' if front else 'haunch',upper=key+'-upper',lower=key+'-lower',foot=key+'-paw',pole=[sign*.10,0,2 if front else -2],sole=.16,contactOffset=[p[i]-h[i] for i in range(3)]))
 # A neutral authored stance isolates bind proportions from the old rearing pose.
 poses={j['id']:dict(offset=[0,0,0],rotation=[0,0,0],scale=[1,1,1]) for j in rig()}
 poses.update({key:dict(offset=[0,0,0],rotation=[0,0,0],scale=[1,1,1]) for key in ['mane','beard','horns','mantle','trim']})
 c['phrases']=[dict(id='lion-neutral',duration=12,samples=[dict(time=t,poses=poses) for t in (0,12)],
   contacts=[dict(chain=key,keys=[dict(time=t,position=p,planted=True) for t in (0,12)]) for key,front,sign,a,k,h,p in legs()],references=[])]
 c['arrangement']=dict(clip='procession',duration=12,looping=False,layers=[dict(id='neutral',phrase='lion-neutral',start=0,end=12,sourceStart=0,sourceEnd=12,fadeIn=0,fadeOut=0,weight=1,additive=False)])
 c['masses']=[dict(joint='body',center=[0,1.8,.2],kilograms=150),dict(joint='mask',center=[0,2.45,-2.1],kilograms=22)]
 c['balance']=dict(joint='body',strength=0,maximumShift=.2)
 collisions=[dict(id='ribcage',joint='spine-1',a=[0,1.82,-.7],b=[0,1.82,.48],radius=.54,ignore=[]),
             dict(id='neck',joint='neck',a=[0,2.15,-1.06],b=[0,2.45,-1.72],radius=.34,ignore=[])]
 result=s.command('author',expectedRevision=snapshot['revision'],craft=c,contactOffsets={},collisionBodies=collisions,
   surfaceEdits=[x for x in old.get('surfaceEdits',[]) if x['part'] not in changed],
   surfaceLayers=[x for x in old.get('surfaceLayers',[]) if x['part'] not in changed])
 s.command('surfaceReview',mode='clay',hiddenParts=['mane','beard','horns','mantle','trim','tail'])
 s.command('creature',mode='procession',seconds=0)
 s.command('pause',value=True)
 s.command('view',value='quarter');s.command('camera',metres=6)
 print(json.dumps({'revision':result.get('revision'),'anatomy':[x['part'] for x in new]}))
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--write',type=Path);args=parser.parse_args()
 if args.write:args.write.write_text(json.dumps(dict(anatomy=list(anatomy()),joints=rig()),indent=2)+'\n')
 else:apply()
