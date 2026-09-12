"""Apply Vesper's dense-clump groom study through the public authoring interface.

Run with Sanctuary / Vesper already selected. This is one undoable transaction;
it does not publish an asset or alter existing review baselines.
"""
import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'Tools' / 'AgentTools'))
from creature import CreatureWorkspace

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--contacts-only', action='store_true',
                    help='Fit conservative discrete guide capsules to the current groom envelopes')
parser.add_argument('--flow', action='store_true',
                    help='Author an uneven falling collar and softer subdivided tip masses')
args = parser.parse_args()
studio = CreatureWorkspace()
if args.flow:
    snapshot = studio.command('authoring')['authoring']
    source = snapshot['source']
    if source.get('id') != 'vesper':
        raise RuntimeError('Select Sanctuary / Vesper before authoring its collar flow')
    craft = dict(source['craft'])
    nodes = {n['id']: n['position'] for n in snapshot['secondaryRig']['nodes']}
    offsets = dict(source.get('guideOffsets', {}))
    designs = []
    def authored_length(index, count, profile):
        """Resample an authored length rhythm for the current guide topology."""
        position = index/max(1,count-1)*(len(profile)-1)
        left = min(len(profile)-2,int(position))
        blend = position-left
        return profile[left]*(1-blend)+profile[left+1]*blend
    lengths = [1.10,.76,.94,.82,1.03,.79,.96,.88,1.05,.84,.98,.81,1.02,.85,.73,1.12]
    for original in craft['grooms']:
        groom = dict(original)
        if groom['part'] not in ('mane','beard'):
            designs.append(groom)
            continue
        groom.update(width=.195 if groom['part']=='mane' else .092,
                     curl=.004, radius=.00135, fibres=56 if groom['part']=='mane' else 36,
                     lengthVariation=.21, flyaways=.045,
                     envelope=dict(coverage=1.03,taper=1.08,flatten=.93,ridge=.018),
                     rootColor=[.40,.35,.27],tipColor=[.64,.57,.455])
        if groom['part']=='mane':
            for i,guide in enumerate(groom['guides']):
                angle = -.25+i*(math.pi+.5)/max(1,len(groom['guides'])-1)
                c,s = math.cos(angle),math.sin(angle)
                flank = 1-max(0,s)
                root = nodes[guide[0]]
                long = authored_length(i,len(groom['guides']),lengths)
                deltas = [[c*.19,s*.075-.035,.12],
                          [c*(.25+.08*s),(-.28-.25*flank)*long,(.38+.12*s)*long],
                          [c*(.26+.07*s),(-.88-.22*flank)*long,(.68+.35*s)*long]]
                for j,node in enumerate(guide[1:]):
                    # The guide may have more or fewer than four nodes. Sample
                    # the same authored source curve across its complete span.
                    control = [[0,0,0]]+deltas
                    parameter = (j+1)/max(1,len(guide)-1)*(len(control)-1)
                    left = min(len(control)-2,int(parameter))
                    blend = parameter-left
                    delta = [control[left][k]*(1-blend)+control[left+1][k]*blend for k in range(3)]
                    base = [nodes[node][k]-offsets.get(node,[0,0,0])[k] for k in range(3)]
                    offsets[node] = [round(root[k]+delta[k]-base[k],6) for k in range(3)]
        else:
            for i,guide in enumerate(groom['guides']):
                root = nodes[guide[0]]
                long = authored_length(i,len(groom['guides']),[.78,1.00,.91,.71])
                deltas = [[root[0]*.08,-.23,-.045],
                          [root[0]*.20,-.51*long,.02],
                          [root[0]*.26,-.78*long,.19]]
                for j,node in enumerate(guide[1:]):
                    control = [[0,0,0]]+deltas
                    parameter = (j+1)/max(1,len(guide)-1)*(len(control)-1)
                    left = min(len(control)-2,int(parameter))
                    blend = parameter-left
                    delta = [control[left][k]*(1-blend)+control[left+1][k]*blend for k in range(3)]
                    base = [nodes[node][k]-offsets.get(node,[0,0,0])[k] for k in range(3)]
                    offsets[node] = [round(root[k]+delta[k]-base[k],6) for k in range(3)]
        designs.append(groom)
    craft['grooms'] = designs
    studio.command('author',expectedRevision=snapshot['revision'],craft=craft,guideOffsets=offsets)
    print(json.dumps({'flowStudy': 'Uneven falling collar', 'published':False,
                      'next':'Refit contact proxies with --contacts-only, then review actual motion'}))
    raise SystemExit(0)

if args.contacts_only:
    snapshot = studio.command('authoring')['authoring']
    source = snapshot['source']
    if source.get('id') != 'vesper':
        raise RuntimeError('Select Sanctuary / Vesper before fitting its groom proxies')
    designs = [g for g in source['craft']['grooms'] if g['part'] in ('mane', 'beard')]
    if len(designs) != 2 or any('envelope' not in g for g in designs):
        raise RuntimeError('Apply the Vesper finery groom study before fitting contact proxies')
    nodes = {n['id']: n['position'] for n in snapshot['secondaryRig']['nodes']}
    radii = dict(source.get('guideRadii', {}))
    def curve(points, t):
        n = len(points)-1
        f = min(n*t, n-.00001)
        i, u = int(f), f-int(f)
        a,b,c,d = points[max(0,i-1)],points[i],points[min(n,i+1)],points[min(n,i+2)]
        return [.5*(2*b[k]+(c[k]-a[k])*u+(2*a[k]-5*b[k]+4*c[k]-d[k])*u*u
                       +(-a[k]+3*b[k]-3*c[k]+d[k])*u*u*u) for k in range(3)]
    def distance_to_segment(p, a, b):
        d = [b[k]-a[k] for k in range(3)]
        u = max(0,min(1,sum((p[k]-a[k])*d[k] for k in range(3))/sum(x*x for x in d)))
        return math.sqrt(sum((p[k]-a[k]-d[k]*u)**2 for k in range(3)))
    changed = {}
    for groom in designs:
        envelope = groom['envelope']
        for guide in groom['guides']:
            points = [nodes[node] for node in guide]
            bounds = [0.001] * len(guide)
            for segment in range(len(guide)-1):
                extent = .001
                for sample in range(33):
                    t = (segment+sample/32)/(len(guide)-1)
                    radial = groom['width']*envelope['coverage']*(.84+.26*math.sin(t*math.pi))*pow(max(0,1-t),envelope['taper'])
                    radial *= 1+envelope['ridge']*1.30
                    wave = groom['curl']*math.sin(t*math.pi)
                    flyaway = groom['width']*.18*math.sin(t*math.pi) if groom['flyaways'] else 0
                    shell = radial+wave+flyaway+groom['radius']*2.5+.003
                    extent = max(extent,distance_to_segment(curve(points,t),points[segment],points[segment+1])+shell)
                # Both endpoints carry this span's upper bound, making the
                # solver's tapered capsule conservative for this sampled curve.
                bounds[segment] = max(bounds[segment],extent)
                bounds[segment+1] = max(bounds[segment+1],extent)
            for node, radius in zip(guide,bounds):
                radii[node] = changed[node] = round(radius,5)
    studio.command('author', expectedRevision=snapshot['revision'], guideRadii=radii)
    print(json.dumps({'contactProxyStudy': True, 'nodes': len(changed),
                      'minimumRadius': min(changed.values()), 'maximumRadius': max(changed.values()),
                      'published': False,
                      'scope': 'Sampled bind-space curve envelopes; audit deformed surfaces and live motion separately'}))
    raise SystemExit(0)

with studio.edit() as edit:
    designs = {item['part']: item for item in edit.snapshot['source']['grooms']}
    if not all(part in designs for part in ('mane', 'beard')):
        raise RuntimeError('Select Sanctuary / Vesper before applying this finery study')
    for part in ('mane', 'beard'):
        groom = dict(designs[part])
        groom.update(
            fibres=48 if part == 'mane' else 32,
            width=.17 if part == 'mane' else .088,
            radius=.0018 if part == 'mane' else .0015,
            curl=.007 if part == 'mane' else .005,
            frequency=1.1, clump=.78, lengthVariation=.13, flyaways=.025,
            rootColor=[.42, .365, .275], tipColor=[.70, .625, .49],
            envelope=dict(coverage=.96, taper=.42, flatten=.72, ridge=.045))
        edit.upsert('grooms', groom)
print(json.dumps({'applied': ['mane', 'beard'], 'published': False}))
