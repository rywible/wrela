# Wrela Charter: the MMO's display face, made from IM FELL English SC (Igino Marini's revival
# of the Fell types, SIL Open Font License 1.1, its Reserved Font Name kept off this one): a
# crossed W (two of its Vs overlapped, as an engraver's W is) and an R whose leg runs on into a
# long swash under the next letter, rough-edged as the face's own type is.
#
#   pip install fonttools skia-pathops
#   python3 charter.py 880 IM_FELL_English_SC.ttf WrelaCharter.ttf
#
# (IM_FELL_English_SC.ttf from https://github.com/google/fonts/tree/main/ofl/imfellenglishsc.)
import sys
from fontTools.ttLib import TTFont
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.cu2quPen import Cu2QuPen
import pathops

import os
SRC=sys.argv[2] if len(sys.argv)>2 else '../fonts/IM_Fell_English_SC.ttf'
OUT=sys.argv[3] if len(sys.argv)>3 else 'WrelaCharter.ttf'
f=TTFont(SRC)
gs=f.getGlyphSet()
glyf=f['glyf']; hmtx=f['hmtx']

def path_of(name, dx=0, dy=0):
    p=pathops.Path()
    pen=p.getPen()
    gs[name].draw(TransformPen(pen,(1,0,0,1,dx,dy)))
    return p

def union(*paths):
    out=pathops.Path()
    b=pathops.OpBuilder(fix_winding=True, keep_starting_points=False)
    for p in paths: b.add(p, pathops.PathOp.UNION)
    return b.resolve()

def put(name, path, width):
    tp=TTGlyphPen(gs)
    path.draw(Cu2QuPen(tp, 1.0, reverse_direction=False))
    g=tp.glyph()
    glyf[name]=g
    g.recalcBounds(glyf)
    hmtx[name]=(width, g.xMin)

# W: two of the face's Vs, overlapped so the inner strokes cross, as an engraver's W is.
DX=int(sys.argv[1]) if len(sys.argv)>1 else 880
put('W', union(path_of('V'), path_of('V', DX)), gs['V'].width + DX)

# R: its leg carried on into a long swash under the next letter, thinning to a hairline.
import math, random
random.seed(11)
def bez(p0,p1,p2,p3,t):
    u=1-t
    return tuple(u**3*a+3*u*u*t*b+3*u*t*t*c+t**3*d for a,b,c,d in zip(p0,p1,p2,p3))
# The swash's spine: out of the leg's foot, down under the baseline, and on, rising to a point.
P=[(1330,170),(1470,-200),(1900,-275),(2480,-170)]
N=40
spine=[bez(*P,i/N) for i in range(N+1)]
left=[];right=[]
for i,(x,y) in enumerate(spine):
    t=i/N
    if i<N:
        nx,ny=spine[i+1][0]-x, spine[i+1][1]-y
    else:
        nx,ny=x-spine[i-1][0], y-spine[i-1][1]
    l=math.hypot(nx,ny); nx,ny=nx/l,ny/l
    # thick out of the foot (as the leg is), thinning to a hairline; its edges a little rough, as
    # the face's type is.
    w=(85*(1-t)**1.1+16)
    j=lambda: random.uniform(-7,7)*(1-t*0.7)
    left.append((x-ny*w+j(), y+nx*w+j()))
    right.append((x+ny*w*0.8+j(), y-nx*w*0.8+j()))
tail=pathops.Path(); pen=tail.getPen()
pts=left+right[::-1]
pen.moveTo(pts[0])
for q in pts[1:]: pen.lineTo(q)
pen.closePath()
put('R', union(path_of('R'), tail), gs['R'].width + 70)

for rec in f['name'].names:
    s=rec.toUnicode()
    s=s.replace('IM FELL English SC','Wrela Charter').replace('IMFELLEnglishSC','WrelaCharter')
    if rec.nameID==0:
        s='Wrela Charter: © 2007 Igino Marini (www.iginomarini.com), as IM FELL English SC; modified 2026 for Wrela (a crossed W, a swash R). SIL Open Font License 1.1.'
    rec.string=s
f.save(OUT)

# Kept to the Latin-1 the baker reads (ui/ttf.wrela): the Fell's rough outlines are many points
# a glyph, and every glyph baked is a build's time.
from fontTools import subset
opts = subset.Options()
opts.name_IDs = ['*']
opts.name_languages = ['*']
opts.notdef_outline = True
opts.layout_features = ['kern']
sub = subset.Subsetter(opts)
sub.populate(unicodes=list(range(0x20, 0x7F)) + list(range(0xA0, 0x100)))
ft = TTFont(OUT)
sub.subset(ft)
ft.save(OUT)
