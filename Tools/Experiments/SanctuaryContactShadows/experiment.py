#!/usr/bin/env python3
"""Bounded CPU shadow-projection model; never a renderer or mesh validation.

Uses exact source bench primitives on a locally fitted terrain plane, a sampled
directional depth map, bilinear comparison and receiver-plane corrected PCF.
The analytic tangent models raster slope; actual extracted triangles/GPU depth
rounding are intentionally not claimed equivalent. Standard library only.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import time
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
TEXEL = 130 / 2048
DEPTH_RANGE = 249
BIAS_CONSTANT = 0.00005 * DEPTH_RANGE
RASTER_CLAMP = 0.001 * DEPTH_RANGE
ORIGIN = (0.78509414, 1.410673, 20.149673)
SOURCE_GROUND = (1.404460456, 1.416394791)
GROUND_Z_SLOPE = (SOURCE_GROUND[1] - SOURCE_GROUND[0]) / 1.36
GROUND_Y = sum(SOURCE_GROUND) / 2 - ORIGIN[1]


def dot(a, b): return sum(x * y for x, y in zip(a, b))
def add(a, b): return tuple(x + y for x, y in zip(a, b))
def mul(a, s): return tuple(x * s for x in a)
def sub(a, b): return add(a, mul(b, -1))
def unit(a): return mul(a, 1 / math.sqrt(dot(a, a)))
def cross(a, b):
    return (a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0])


def sphere_hits(q, direction, center, radius):
    delta = sub(q, center)
    b = dot(delta, direction)
    discriminant = b*b - dot(delta, delta) + radius*radius
    if discriminant < 0: return []
    return [(d, unit(sub(add(q, mul(direction, d)), center)))
            for d in (-b-math.sqrt(discriminant), -b+math.sqrt(discriminant))]


def capsule_hit(q, direction, z):
    """Vertical source capsule: endpoints y .04/.51, radius .055 m."""
    r, low, high = .055, .04, .51
    candidates = []
    a = direction[0]**2 + direction[2]**2
    b = q[0]*direction[0] + (q[2]-z)*direction[2]
    c = q[0]**2 + (q[2]-z)**2-r*r
    if a > 1e-12 and b*b-a*c >= 0:
        for d in ((-b-math.sqrt(b*b-a*c))/a, (-b+math.sqrt(b*b-a*c))/a):
            p = add(q, mul(direction, d))
            if low <= p[1] <= high:
                candidates.append((d, unit((p[0], 0, p[2]-z))))
    for y, lower in ((low, True), (high, False)):
        for d, normal in sphere_hits(q, direction, (0, y, z), r):
            py = q[1]+direction[1]*d
            if (py <= low if lower else py >= high): candidates.append((d, normal))
    return min(candidates, key=lambda x: x[0]) if candidates else None


def box_hit(q, direction, center, radii):
    entry, leave, normal = -math.inf, math.inf, None
    for axis in range(3):
        if abs(direction[axis]) < 1e-12:
            if abs(q[axis]-center[axis]) > radii[axis]: return None
            continue
        first = (center[axis]-radii[axis]-q[axis])/direction[axis]
        last = (center[axis]+radii[axis]-q[axis])/direction[axis]
        sign = -1
        if first > last: first, last, sign = last, first, 1
        if first > entry:
            entry = first
            normal = tuple(sign if i == axis else 0 for i in range(3))
        leave = min(leave, last)
        if entry > leave: return None
    return entry, normal


class Projection:
    def __init__(self, altitude=36, slope_x=0, slope_z=GROUND_Z_SLOPE,
                 raster_slope=1, radius=2, phase=(0, 0), full_bench=True):
        a, z = math.radians(altitude), math.radians(-35)
        self.sun = (math.cos(a)*math.sin(z), math.sin(a), -math.cos(a)*math.cos(z))
        self.right = unit(cross((0, 1, 0), self.sun))
        # Texture V reverses clip Y, matching Surface.metal's (.5,-.5).
        self.up = mul(cross(self.sun, self.right), -1)
        self.direction = mul(self.sun, -1)
        self.normal = unit((-slope_x, 1, -slope_z))
        self.slope_x, self.slope_z = slope_x, slope_z
        self.raster_slope, self.radius, self.full_bench = raster_slope, radius, full_bench
        self.gradient = self.derivative(self.normal)
        self.receiver_bias = BIAS_CONSTANT + TEXEL*sum(abs(x) for x in self.gradient)
        # World-origin anchoring of the actual orthographic matrix. Local source
        # positions require the corresponding global phase, plus optional sweep.
        self.phase = (dot(self.right, ORIGIN)/TEXEL + phase[0],
                      dot(self.up, ORIGIN)/TEXEL + phase[1])

    def derivative(self, normal):
        denominator = dot(normal, self.sun)
        if abs(denominator) < 1e-10: return (1e10, 1e10)
        return dot(normal, self.right)/denominator, dot(normal, self.up)/denominator

    def raster_bias(self, normal):
        # A representative depth32Float ULP at normalized depth .5. Constant
        # bias is negligible at this scale; this is not Apple's exact rounding.
        constant_ulp = DEPTH_RANGE * 2**-24
        slope = TEXEL * max(abs(v) for v in self.derivative(normal))
        return min(RASTER_CLAMP, constant_ulp+self.raster_slope*slope)

    @lru_cache(maxsize=32768)
    def depth(self, ix, iy):
        u = (ix+.5-self.phase[0])*TEXEL
        v = (iy+.5-self.phase[1])*TEXEL
        q = add(mul(self.right, u), mul(self.up, v))
        d = (dot(self.normal, q)-self.normal[1]*GROUND_Y)/dot(self.normal, self.sun)
        candidates = [(d, self.normal)]
        for z in (-.68, .68):
            hit = capsule_hit(q, self.direction, z)
            if hit: candidates.append(hit)
        if self.full_bench:
            # Exact source boxes after the captured pi/2 yaw.
            for center, radii in (((0,.54,0),(.32,.075,.88)), ((.27,.78,0),(.06,.28,.88))):
                hit = box_hit(q, self.direction, center, radii)
                if hit: candidates.append(hit)
        return min(d+self.raster_bias(n) for d, n in candidates)

    def ground(self, x, z):
        return x, GROUND_Y+self.slope_x*x+self.slope_z*z, z

    def blocked(self, p):
        # Source visibility reference, with no sampled map/filter/bias.
        receiver = dot(self.direction, p)
        u, v = dot(self.right, p), dot(self.up, p)
        q = add(mul(self.right, u), mul(self.up, v))
        hits = [capsule_hit(q, self.direction, z) for z in (-.68, .68)]
        if self.full_bench:
            hits += [box_hit(q, self.direction, c, r) for c,r in
                     (((0,.54,0),(.32,.075,.88)), ((.27,.78,0),(.06,.28,.88)))]
        return any(h and h[0] < receiver-1e-8 for h in hits)

    def occlusion(self, p):
        u, v, d = dot(self.right,p), dot(self.up,p), dot(self.direction,p)
        total = 0
        pixel_x, pixel_y = u/TEXEL+self.phase[0]-.5, v/TEXEL+self.phase[1]-.5
        if self.radius == 2:
            # Actual compact 9-fetch paired branch, including its per-fetch
            # receiver-plane reference (not an assumption of 25-fetch identity).
            def paired(pixel):
                cell = math.floor(pixel); f = pixel-cell
                weights = (2-f,2,1+f)
                offsets = (-2+1/weights[0],.5,2+f/weights[2])
                return [(cell+o-pixel,w) for o,w in zip(offsets,weights)]
            xs,ys = paired(pixel_x),paired(pixel_y)
        else:
            xs=ys=[(i,1) for i in range(-self.radius,self.radius+1)]
        for oy, sample_wy in ys:
            for ox, sample_wx in xs:
                x, y = pixel_x+ox, pixel_y+oy
                ix, iy = math.floor(x), math.floor(y)
                fx, fy = x-ix, y-iy
                compare = d+TEXEL*(self.gradient[0]*ox+self.gradient[1]*oy)-self.receiver_bias
                for dx,wx in ((0,1-fx),(1,fx)):
                    for dy,wy in ((0,1-fy),(1,fy)):
                        total += sample_wx*sample_wy*wx*wy*(compare > self.depth(ix+dx,iy+dy))
        return total/(2*self.radius+1)**2


def main():
    started=time.perf_counter()
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,default=Path('.build/contact-shadow-experiment'))
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    variants={'reference':(1,2),'raster-slope-zero':(0,2),'compact-3x3':(1,1)}
    models={k:Projection(raster_slope=s,radius=r) for k,(s,r) in variants.items()}
    reference=models['reference']
    downstream=unit((-reference.sun[0],0,-reference.sun[2]))
    rows=[]
    for foot_z in (-.68,.68):
        for step in range(121):
            distance=.015+step*.005
            p=reference.ground(downstream[0]*distance,foot_z+downstream[2]*distance)
            rows.append({'footZ':foot_z,'distanceFromFootAxisMetres':distance,
                         'sourceOcclusion':float(reference.blocked(p)),
                         **{k:m.occlusion(p) for k,m in models.items()}})
    with (args.output/'contact-profiles.csv').open('w') as f:
        writer=csv.DictWriter(f,fieldnames=rows[0].keys());writer.writeheader();writer.writerows(rows)
    cases=[]
    for altitude in (12,36,65):
        for slope in (-.25,0,.25):
            m=Projection(altitude=altitude,slope_z=slope)
            cases.append({'altitudeDegrees':altitude,'groundSlopeZ':slope,
                          'receiverBiasLightMetres':m.receiver_bias,
                          'planeRasterBiasLightMetres':m.raster_bias(m.normal),
                          'lightNormalDot':dot(m.normal,m.sun),
                          'planeEquivalentNormalSeparationMetres':
                          (m.receiver_bias+m.raster_bias(m.normal))*abs(dot(m.normal,m.sun))})
    phase_samples={}
    for name,(s,r) in variants.items():
        values=[]
        for px in (0,.25,.5,.75):
            for py in (0,.25,.5,.75):
                m=Projection(raster_slope=s,radius=r,phase=(px,py),full_bench=False)
                p=m.ground(downstream[0]*.08,-.68+downstream[2]*.08)
                values.append(m.occlusion(p))
        phase_samples[name]={'min':min(values),'mean':sum(values)/len(values),'max':max(values)}
    profiles={}
    for name in variants:
        profiles[name]=[]
        for z in (-.68,.68):
            subset=[r for r in rows if r['footZ']==z]
            contact=[r[name] for r in subset if .06-1e-9<=r['distanceFromFootAxisMetres']<=.16+1e-9]
            profiles[name].append({'footZ':z,
                'meanOcclusionAxisDistance60To160mm':sum(contact)/len(contact),
                'firstAtLeast25PercentFromAxisMetres':next((r['distanceFromFootAxisMetres'] for r in subset if r[name]>=.25),None),
                'maximumOcclusion':max(r[name] for r in subset)})
    # Flat/sloped source planes far from both capsules. This checks the model's
    # comparison/derivative signs; it cannot certify extracted mesh acne.
    plane_checks=[]
    for altitude in (12,36,65):
        for slope in (-.25,0,.25):
            for name,(s,r) in variants.items():
                m=Projection(altitude=altitude,slope_z=slope,raster_slope=s,radius=r,full_bench=False)
                values=[m.occlusion(m.ground(5+x*TEXEL/4,5+z*TEXEL/4))
                        for x in range(4) for z in range(4)]
                plane_checks.append({'altitudeDegrees':altitude,'groundSlopeZ':slope,
                                     'variant':name,'maximumFalseOcclusion':max(values)})
    source_paths=['Engine/FieldEngine/FrameComposer.swift','Engine/FieldEngine/CameraMath.swift',
        'Engine/FieldEngine/Rendering/MetalRenderer.swift','Engine/FieldEngine/Resources/Surface.metal',
        'Games/Sanctuary/Project/LivingWorldPresentation.swift']
    result={'model':'analytic source proxy; not compiled mesh, Metal execution, or visual acceptance',
        'worldUnit':'metre','textureSize':2048,'lightSpan':130,'depthRange':DEPTH_RANGE,
        'texelMetres':TEXEL,'nominal5TapFootprintMetres':5*TEXEL,
        'nominal3TapFootprintMetres':3*TEXEL,'legDiameterTexels':.110/TEXEL,
        'legGroundIntersectionDiameterMetres':[2*math.sqrt(.055**2-(.055-penetration)**2)
            for penetration in (.008787456,.020721791)],
        'receiverConstantLightMetres':BIAS_CONSTANT,'rasterClampUpperBoundLightMetres':RASTER_CLAMP,
        'nativeSun':list(reference.sun),'nativeGroundZFit':GROUND_Z_SLOPE,
        'nativeReceiverBiasLightMetres':reference.receiver_bias,
        'nativePlaneRasterBiasLightMetres':reference.raster_bias(reference.normal),
        'profiles':profiles,'isolatedLeg80mmPhaseSweep':phase_samples,'slopeCases':cases,
        'planeComparisonSanity':plane_checks,'cpuSeconds':time.perf_counter()-started,
        'bounds':{'profilePoints':len(rows),'phasePointsPerVariant':16,
                  'planeSanityPoints':len(plane_checks)*16,'maximumCachedDepthSamples':32768},
        'sourceDigests':{p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in source_paths}}
    (args.output/'results.json').write_text(json.dumps(result,indent=2)+'\n')
    # An engineering scale diagram, not a synthetic renderer image.
    scale=1500
    strips=[('5-tap nominal light-plane footprint',5*TEXEL),('3-tap nominal footprint',3*TEXEL),
            ('Source leg maximum diameter',.110),('One shadow texel',TEXEL)]
    svg=['<svg xmlns="http://www.w3.org/2000/svg" width="920" height="330" viewBox="0 0 920 330">',
         '<rect width="920" height="330" fill="#fff"/>',
         '<text x="25" y="32" font-family="sans-serif" font-size="20">Contact scale · 130 m / 2048 directional map</text>']
    for i,(label,width) in enumerate(strips):
        y=62+i*55
        svg += [f'<text x="25" y="{y+17}" font-family="sans-serif" font-size="14">{label}</text>',
                f'<rect x="335" y="{y}" width="{width*scale}" height="25" fill="#{["e59b54","deb968","5ba7b1","8196b6"][i]}"/>',
                f'<text x="{345+width*scale}" y="{y+18}" font-family="sans-serif" font-size="13">{width*1000:.1f} mm</text>']
    svg+=['<text x="25" y="305" font-family="sans-serif" font-size="13">Scale diagram only. Plane projection, filtering and bias change actual contact visibility.</text>','</svg>']
    (args.output/'footprint.svg').write_text('\n'.join(svg))
    print(json.dumps({'output':str(args.output),'texelMM':TEXEL*1000,
                      'profiles':profiles,'phaseSweep':phase_samples},indent=2))


if __name__=='__main__': main()
