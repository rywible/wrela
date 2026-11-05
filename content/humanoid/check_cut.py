#!/usr/bin/env python3
"""Automated checks for the classic cut (SHOW=15 ONE=1), run before any visual critique.

Build with --set PROBE=4 and run wrela-host; each frame prints a `P,...` line of world positions
(see main.wrela `probe_frame`). This reads that log and checks, against the cut's frame data
(mixamo.wrela `V_FRAMES`):

  timing    raise, jodan hold, snap, blow frame, hit-stop, all on twos
  grip      both hands on the hilt, right at the guard, left toward the pommel
  feet      a planted foot doesn't slide
  joints    knees and elbows bend within limits, knees forward
  clipping  the blade and hilt stay out of the body (arms aside)
  contact   the blade meets the dummy's head at the blow

Usage: check_cut.py probe.log [--fps 60]. Exits 1 if any check fails.
"""
import sys, math

FRAMES = dict(raise_=15, hold=18, snap=6, active=3, stop=12, sink=8, zanshin=8, back=18)
NAMES = ["grip", "guard", "pommel", "tip", "right", "left", "lToes", "lFoot", "rToes", "rFoot",
         "lThigh", "lShin", "rThigh", "rShin", "lUpperArm", "lLowerArm", "lHand",
         "rUpperArm", "rLowerArm", "rHand", "head", "neck", "upperChest", "chest", "spine",
         "hips", "dummyHead"]


def sub(a, b): return (a[0] - b[0], a[1] - b[1], a[2] - b[2])
def add(a, b): return (a[0] + b[0], a[1] + b[1], a[2] + b[2])
def mul(a, k): return (a[0] * k, a[1] * k, a[2] * k)
def dot(a, b): return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
def norm(a): return math.sqrt(dot(a, a))


def seg_point(p, a, b):
    ab = sub(b, a)
    t = max(0.0, min(1.0, dot(sub(p, a), ab) / max(dot(ab, ab), 1e-12)))
    return norm(sub(p, add(a, mul(ab, t)))), t


def seg_seg(a, b, c, d, n=24):
    return min(seg_point(add(a, mul(sub(b, a), i / n)), c, d)[0] for i in range(n + 1))


def angle(a, b, c):
    u, v = sub(a, b), sub(c, b)
    return math.degrees(math.acos(max(-1, min(1, dot(u, v) / (norm(u) * norm(v) + 1e-12)))))


def read(path):
    rows = []
    for line in open(path):
        i = line.find("P,")
        if i < 0:
            continue
        f = line[i + 2:].strip().split(",")
        r = dict(time=float(f[0]), acting=int(f[1]), act=float(f[2]), drawn=float(f[3]))
        vals = list(map(float, f[4:]))
        for k, n in enumerate(NAMES):
            r[n] = tuple(vals[k * 3:k * 3 + 3])
        rows.append(r)
    return rows


def main():
    rows = read(sys.argv[1])
    fails, notes = [], []
    # (A one-off's first frame is already a frame into it: its time 1/60 there.)
    start = next(i for i, r in enumerate(rows) if r["acting"]) - 1
    end = next((i for i in range(start + 1, len(rows)) if not rows[i]["acting"]), len(rows))
    moved = [0.0] + [max(norm(sub(rows[i][k], rows[i - 1][k])) for k in ("tip", "grip", "hips", "head", "lToes", "rToes"))
                     for i in range(1, len(rows))]
    tipmv = [0.0] + [norm(sub(rows[i]["tip"], rows[i - 1]["tip"])) for i in range(1, len(rows))]
    F = lambda i: i - start  # frames into the cut

    # --- timing ---
    up = start + FRAMES["raise_"]
    go = up + FRAMES["hold"]
    blow = go + FRAMES["snap"]
    hold_drift = sum(tipmv[up + 1:go])
    notes.append(f"jodan hold frames {F(up)}-{F(go)}: tip drift {hold_drift * 1000:.1f} mm")
    if hold_drift > 0.02:
        fails.append(f"timing: jodan hold drifts {hold_drift * 1000:.0f} mm (want < 20)")
    # The blow: the first frame after the hold where the tip stops dead.
    # (On twos every other frame is still: dead still means four frames running.)
    contact = next((i for i in range(go - 3, end - 5) if tipmv[i] > 0.05 and max(tipmv[i + 1:i + 5]) < 1e-4), None)
    if contact is None:
        fails.append("timing: no blow found (the tip never stops dead after the snap)")
    else:
        notes.append(f"blow on frame {F(contact)} (table {F(blow)})")
        if abs(contact - blow) > 1:
            fails.append(f"timing: blow on frame {F(contact)}, table says {F(blow)}")
        still = 0
        while contact + 1 + still < end and moved[contact + 1 + still] < 1e-4:
            still += 1
        notes.append(f"hit-stop held {still + 1} frames (table {FRAMES['stop']})")
        if abs(still + 1 - FRAMES["stop"]) > 1:
            fails.append(f"timing: hit-stop {still + 1} frames, table says {FRAMES['stop']}")
        snapped = [i for i in range(go, contact + 1) if tipmv[i] > 0.01]
        notes.append(f"snap drawings: {len(set(round(rows[i]['drawn'], 3) for i in snapped))}, tip speeds (m/frame) " +
                     " ".join(f"{tipmv[i]:.2f}" for i in snapped))
    ones = [F(i) for i in range(start + 1, end - 1)
            if moved[i] > 1e-4 and moved[i + 1] > 1e-4 and abs(rows[i]["drawn"] - rows[i + 1]["drawn"]) > 1e-4
            and abs(rows[i]["drawn"] - rows[i - 1]["drawn"]) > 1e-4]
    if ones:
        fails.append(f"timing: drawings held one frame only (not on twos) at frames {ones[:10]}")

    # --- grip, joints, clipping, feet ---
    worst_grip, worst_clip, slides, joints = 0.0, (1.0, None, None), [], []
    body = [("head", 0.1), ("neck", 0.06), ("upperChest", 0.13), ("chest", 0.13), ("spine", 0.12), ("hips", 0.13)]
    legs = [("lThigh", "lShin", 0.08), ("rThigh", "rShin", 0.08), ("lShin", "lFoot", 0.055), ("rShin", "rFoot", 0.055)]
    for i in range(start, len(rows)):
        r = rows[i]
        for hand in ("right", "left"):
            d, _ = seg_point(r[hand], r["guard"], r["pommel"])
            worst_grip = max(worst_grip, d)
            if d > 0.03:
                fails.append(f"grip: {hand} hand {d * 100:.1f} cm off the hilt on frame {F(i)}")
        _, tr = seg_point(r["right"], r["guard"], r["pommel"])
        _, tl = seg_point(r["left"], r["guard"], r["pommel"])
        if tr > tl:
            fails.append(f"grip: right hand below the left on frame {F(i)}")
        span = norm(sub(r["right"], r["left"]))
        if span < 0.17:
            fails.append(f"grip: hands bunched {span * 100:.0f} cm apart on frame {F(i)}")
        # The blade's head piece raised to the skull's middle, a little above the head joint.
        for name, rad in body:
            c = add(r[name], (0, 0.09, 0)) if name == "head" else r[name]
            d = seg_point(c, r["guard"], r["tip"])[0] - rad
            d2 = seg_point(c, r["pommel"], r["guard"])[0] - rad
            m = min(d, d2)
            if m < worst_clip[0]:
                worst_clip = (m, name, F(i))
        for a, b, rad in legs:
            m = seg_seg(r["guard"], r["tip"], r[a], r[b]) - rad
            if m < worst_clip[0]:
                worst_clip = (m, a, F(i))
        for side in "lr":
            knee = angle(r[side + "Thigh"], r[side + "Shin"], r[side + "Foot"])
            bend = 180 - knee
            if bend > 140:
                joints.append(f"{side} knee bent {bend:.0f} deg on frame {F(i)}")
            # A knee bends forward: ahead of the line from hip to ankle (the body faces +z).
            hip, kn, an = r[side + "Thigh"], r[side + "Shin"], r[side + "Foot"]
            d, t = seg_point(kn, hip, an)
            line = add(hip, mul(sub(an, hip), t))
            if bend > 8 and kn[2] < line[2] - 0.01:
                joints.append(f"{side} knee bends backward on frame {F(i)}")
            elbow = 180 - angle(r[side + "UpperArm"], r[side + "LowerArm"], r[side + "Hand"])
            if elbow > 150:
                joints.append(f"{side} elbow bent {elbow:.0f} deg on frame {F(i)}")
        if i > start:
            p = rows[i - 1]
            for side in "lr":
                toe, heel = side + "Toes", side + "Foot"
                down = r[toe][1] < 0.035 and p[toe][1] < 0.035
                slid = math.hypot(r[toe][0] - p[toe][0], r[toe][2] - p[toe][2])
                if down and slid > 0.004:
                    slides.append((F(i), side, slid))
    if worst_clip[0] < 0:
        fails.append(f"clipping: blade {-worst_clip[0] * 100:.1f} cm into the {worst_clip[1]} on frame {worst_clip[2]}")
    notes.append(f"blade's least clearance from the body {worst_clip[0] * 100:.1f} cm ({worst_clip[1]}, frame {worst_clip[2]})")
    notes.append(f"grip: hands at most {worst_grip * 100:.1f} cm off the hilt")
    if slides:
        total = {}
        for f, side, d in slides:
            total[side] = total.get(side, 0) + d
        fails.append("feet: planted toes slide " + ", ".join(f"{s} {d * 100:.1f} cm" for s, d in total.items()) +
                     f" (frames {sorted(set(f for f, _, _ in slides))[:12]})")
    fails += sorted(set(joints))[:8]

    # --- contact ---
    if contact is not None:
        r = rows[contact]
        d, t = seg_point(r["dummyHead"], r["guard"], r["tip"])
        notes.append(f"at the blow the blade passes {d * 100:.1f} cm from the dummy head's middle, {t:.2f} of the way to the point")
        if not (0.06 <= d <= 0.15):
            fails.append(f"contact: blade {d * 100:.0f} cm from the dummy head's middle (want it at the skull, 6-15 cm)")

    print("\n".join("  " + n for n in notes))
    if fails:
        print("FAIL")
        for f in sorted(set(fails), key=fails.index)[:25]:
            print("  - " + f)
        sys.exit(1)
    print("PASS")


if __name__ == "__main__":
    main()
