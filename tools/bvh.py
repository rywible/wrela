#!/usr/bin/env python3
"""Motion capture into wrela: a BVH file's skeleton and frames, and from them the loops a
locomotion needs, written as a wrela module of constants.

    tools/bvh.py stats <file.bvh>...
    tools/bvh.py loops <out.wrela> <idle.bvh> <walk.bvh> <run.bvh> <side.bvh> <back.bvh>

`stats` prints a clip's frames, its speeds and its foot contacts. `loops` cuts each clip into
loops and writes them: the idle's quietest stretch, and for the walk, the run, the sidestep and
the walk backward the average of their steadiest straight cycles, each from the left foot's
contact to its next, time-normalized to `SAMPLES` samples. A loop keeps every joint's local
rotation (a unit quaternion, x y z w), the hips' height and their sway (m), the distance a cycle
covers (its stride), the time it takes and the way it goes; the yaw of the way the body faces is
taken out, so it plays in place, facing +z. Units are metres; +y is up and the figure faces +z,
its left +x.

The clips read are data (100STYLE, CC BY 4.0: see docs/process.md): parse them, don't run them.
"""

import math
import sys

# The samples a cycle is cut to, and the idle's length (s) and its samples' rate.
SAMPLES = 32
IDLE_SECONDS = 4.0
IDLE_RATE = 15


class Joint:
    def __init__(self, name, parent, offset):
        self.name = name
        self.parent = parent
        self.offset = offset
        self.channels = []
        self.end = None


def parse(text):
    """A BVH file's joints (in file order) and its frames (each a list of floats), and the time a
    frame takes (s)."""
    tokens = text.split()
    i = 0
    joints = []
    stack = []

    def take():
        nonlocal i
        t = tokens[i]
        i += 1
        return t

    while True:
        t = take()
        if t in ("ROOT", "JOINT"):
            name = take()
            parent = stack[-1] if stack else None
            assert take() == "{"
            assert take() == "OFFSET"
            offset = tuple(float(take()) for _ in range(3))
            j = Joint(name, parent, offset)
            assert take() == "CHANNELS"
            n = int(take())
            j.channels = [take() for _ in range(n)]
            joints.append(j)
            stack.append(len(joints) - 1)
        elif t == "End":
            assert take() == "Site"
            assert take() == "{"
            assert take() == "OFFSET"
            joints[stack[-1]].end = tuple(float(take()) for _ in range(3))
            assert take() == "}"
        elif t == "}":
            stack.pop()
        elif t == "MOTION":
            break
    assert take() == "Frames:"
    count = int(take())
    assert take() == "Frame" and take() == "Time:"
    dt = float(take())
    width = sum(len(j.channels) for j in joints)
    values = [float(v) for v in tokens[i:i + count * width]]
    assert len(values) == count * width, "the file has fewer frames than it says"
    frames = [values[k * width:(k + 1) * width] for k in range(count)]
    return joints, frames, dt


# ---- quaternions (x, y, z, w) ------------------------------------------------------------------

def q_axis(axis, degrees):
    h = math.radians(degrees) * 0.5
    s = math.sin(h)
    return (axis[0] * s, axis[1] * s, axis[2] * s, math.cos(h))


def q_mul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return (aw * bx + bw * ax + ay * bz - az * by,
            aw * by + bw * ay + az * bx - ax * bz,
            aw * bz + bw * az + ax * by - ay * bx,
            aw * bw - ax * bx - ay * by - az * bz)


def q_rotate(q, v):
    x, y, z, w = q
    tx = 2 * (y * v[2] - z * v[1])
    ty = 2 * (z * v[0] - x * v[2])
    tz = 2 * (x * v[1] - y * v[0])
    return (v[0] + w * tx + (y * tz - z * ty),
            v[1] + w * ty + (z * tx - x * tz),
            v[2] + w * tz + (x * ty - y * tx))


def q_norm(q):
    n = math.sqrt(sum(c * c for c in q))
    return tuple(c / n for c in q)


AXES = {"X": (1, 0, 0), "Y": (0, 1, 0), "Z": (0, 0, 1)}


def pose(joints, frame, scale):
    """Each joint's local rotation, and the root's position (m), in a frame: the channels in the
    file's order, each rotation applied after the ones before it in the list (BVH's convention:
    the first channel outermost)."""
    k = 0
    local = []
    root = (0.0, 0.0, 0.0)
    for j in joints:
        q = (0.0, 0.0, 0.0, 1.0)
        pos = [0.0, 0.0, 0.0]
        for c in j.channels:
            v = frame[k]
            k += 1
            if c.endswith("position"):
                pos["XYZ".index(c[0])] = v * scale
            else:
                q = q_mul(q, q_axis(AXES[c[0]], v))
        if j.parent is None:
            root = tuple(pos)
        local.append(q_norm(q))
    return local, root


def world(joints, local, root, scale):
    """Each joint's world rotation and position."""
    rot = []
    at = []
    for idx, j in enumerate(joints):
        if j.parent is None:
            rot.append(local[idx])
            at.append(root)
        else:
            p = j.parent
            off = q_rotate(rot[p], tuple(c * scale for c in j.offset))
            at.append(tuple(at[p][c] + off[c] for c in range(3)))
            rot.append(q_norm(q_mul(rot[p], local[idx])))
    return rot, at


def heading(q):
    """The way a rotation turns +z, as a yaw about +y (radians, from +z toward +x)."""
    f = q_rotate(q, (0, 0, 1))
    return math.atan2(f[0], f[2])


def wrap(a):
    while a > math.pi:
        a -= 2 * math.pi
    while a <= -math.pi:
        a += 2 * math.pi
    return a


# ---- reading a clip ------------------------------------------------------------------------------

class Clip:
    def __init__(self, path, scale=0.01):
        joints, frames, dt = parse(open(path).read())
        self.joints = joints
        self.dt = dt
        self.scale = scale
        self.local = []
        self.root = []
        self.at = []
        self.rot = []
        for f in frames:
            local, root = pose(joints, f, scale)
            rot, at = world(joints, local, root, scale)
            self.local.append(local)
            self.root.append(root)
            self.rot.append(rot)
            self.at.append(at)
        self.names = [j.name for j in joints]

    def index(self, name):
        return self.names.index(name)

    def speed(self, k, span=15):
        a = self.root[max(k - span, 0)]
        b = self.root[min(k + span, len(self.root) - 1)]
        frames = min(k + span, len(self.root) - 1) - max(k - span, 0)
        return math.hypot(b[0] - a[0], b[2] - a[2]) / (frames * self.dt)

    def toe_speed(self, toe, k):
        p0 = self.at[max(k - 1, 0)][toe]
        p1 = self.at[k][toe]
        return math.hypot(p1[0] - p0[0], p1[2] - p0[2]) / self.dt

    def contacts(self, foot):
        """The frames the foot comes to rest on the ground: the first of at least 8 frames its toe
        stays low and all but still (a stance), after at least 8 it didn't (a swing)."""
        toe = self.index(foot)
        hs = [a[toe][1] for a in self.at]
        floor = sorted(hs)[len(hs) // 20]
        still = [hs[k] < floor + 0.04 and self.toe_speed(toe, k) < 0.35 for k in range(len(hs))]
        out = []
        k = 8
        while k < len(hs) - 8:
            if still[k] and not any(still[k - 8:k]) and all(still[k:k + 8]):
                out.append(k)
                k += 8
            else:
                k += 1
        return out

    def path_yaw(self, k, half=30):
        """The way the body goes at frame `k`: its hips' path over a second round it."""
        a = self.root[max(k - half, 0)]
        b = self.root[min(k + half, len(self.root) - 1)]
        return math.atan2(b[0] - a[0], b[2] - a[2])

    def facing(self, k, half=30):
        """Where the body faces at frame `k`: its upper chest's heading (the pelvis opens toward a
        sidestep; the chest stays square), averaged over a second round it (its sway taken out)."""
        hips = self.index("Chest4")
        lo, hi = max(k - half, 0), min(k + half, len(self.rot) - 1)
        x = sum(math.sin(heading(self.rot[i][hips])) for i in range(lo, hi + 1))
        z = sum(math.cos(heading(self.rot[i][hips])) for i in range(lo, hi + 1))
        return math.atan2(x, z)

    def path_at(self, k, half=30):
        """The hips' path at frame `k`, smoothed over a second round it (their sway taken out)."""
        lo, hi = max(k - half, 0), min(k + half, len(self.root) - 1)
        n = hi - lo + 1
        return tuple(sum(self.root[i][c] for i in range(lo, hi + 1)) / n for c in range(3))

    def cycles(self):
        """Each cycle from a left contact to the next: (start, end, its speed, how far the way it
        goes turns, its stride)."""
        cs = self.contacts("LeftToe")
        out = []
        for a, b in zip(cs, cs[1:]):
            if b - a < 24 or b - a > 120:
                continue
            ra, rb = self.root[a], self.root[b]
            dist = math.hypot(rb[0] - ra[0], rb[2] - ra[2])
            speed = dist / ((b - a) * self.dt)
            turn = abs(wrap(self.path_yaw(b) - self.path_yaw(a)))
            out.append((a, b, speed, turn, dist))
        return out


def stats(path):
    c = Clip(path)
    cyc = c.cycles()
    speeds = sorted(x[2] for x in cyc)
    print(f"{path}: {len(c.local)} frames at {1 / c.dt:.0f} fps; {len(cyc)} cycles")
    if speeds:
        print(f"  cycle speeds: min {speeds[0]:.2f}, median {speeds[len(speeds) // 2]:.2f}, "
              f"max {speeds[-1]:.2f} m/s")
        straight = [x for x in cyc if x[3] < 0.25]
        print(f"  near-straight cycles (turn < 0.25 rad): {len(straight)}")
        for x in straight[:8]:
            print(f"    frames {x[0]}-{x[1]}: {x[2]:.2f} m/s, stride {x[4]:.2f} m, "
                  f"{(x[1] - x[0]) * c.dt:.2f} s, turn {x[3]:.3f}")
    hips = c.index("Hips")
    hy = [a[hips][1] for a in c.at]
    print(f"  hips height: {min(hy):.3f} to {max(hy):.3f} m")


# ---- loops -------------------------------------------------------------------------------------

def frame_at(c, t):
    """The clip at fractional frame `t`: each joint's local turn and the root, between frames."""
    k = min(int(t), len(c.local) - 2)
    u = t - k
    local = [q_nlerp(a, b, u) for a, b in zip(c.local[k], c.local[k + 1])]
    root = tuple(c.root[k][i] * (1 - u) + c.root[k + 1][i] * u for i in range(3))
    return local, root, k


def q_nlerp(a, b, u):
    if sum(x * y for x, y in zip(a, b)) < 0:
        b = tuple(-x for x in b)
    return q_norm(tuple(x * (1 - u) + y * u for x, y in zip(a, b)))


def in_place(c, t):
    """The pose at fractional frame `t` in place: the hips' turn with the yaw of the way the body
    faces taken out, and their place from their smoothed path (their height kept), in the frame of
    the way it faces (so a sidestep stays a sidestep)."""
    local, root, k = frame_at(c, t)
    yaw = c.facing(k)
    unturn = q_axis((0, 1, 0), -math.degrees(yaw))
    local = list(local)
    local[0] = q_norm(q_mul(unturn, local[0]))
    path = c.path_at(k)
    d = (root[0] - path[0], 0.0, root[2] - path[2])
    d = q_rotate(unturn, d)
    return local, (d[0], root[1], d[2])


def average(samples):
    """The average of several cycles' samples (each a list of (local turns, hips)): each turn
    signed to the first's, summed and normalized; the hips averaged."""
    n = len(samples)
    out = []
    for i in range(len(samples[0])):
        turns = []
        for j in range(len(samples[0][i][0])):
            ref = samples[0][i][0][j]
            acc = [0.0, 0.0, 0.0, 0.0]
            for s in samples:
                q = s[i][0][j]
                if sum(x * y for x, y in zip(q, ref)) < 0:
                    q = tuple(-x for x in q)
                acc = [a + b for a, b in zip(acc, q)]
            turns.append(q_norm(tuple(acc)))
        hips = tuple(sum(s[i][1][c] for s in samples) / n for c in range(3))
        out.append((turns, hips))
    return out


def q_inverse(q):
    return (-q[0], -q[1], -q[2], q[3])


def close(samples):
    """A loop's samples (SAMPLES + 1 of them, the last the first's next) closed: what the last
    misses the first by, spread back over the loop, so it plays round without a jump. The last is
    dropped."""
    first, last = samples[0], samples[-1]
    n = len(samples) - 1
    out = []
    for i in range(n):
        u = i / n
        turns = []
        for j, q in enumerate(samples[i][0]):
            err = q_mul(first[0][j], q_inverse(last[0][j]))
            fix = q_nlerp((0.0, 0.0, 0.0, 1.0), err, u)
            turns.append(q_norm(q_mul(fix, q)))
        hips = tuple(samples[i][1][c] + (first[1][c] - last[1][c]) * u for c in range(3))
        out.append((turns, hips))
    return out


def going(c, a, b):
    """The way a cycle from frame `a` to `b` goes from the way the body faces (rad, toward its
    left positive)."""
    go = math.atan2(c.root[b][0] - c.root[a][0], c.root[b][2] - c.root[a][2])
    return wrap(go - c.facing((a + b) // 2))


def cycle_loop(c, aim=0.0, count=6):
    """A cycle's loop: the average of its `count` fastest near-straight cycles going within 15° of
    `aim` (degrees from the way the body faces: a sidestep clip goes both ways), each from the
    left foot's contact to its next. The clips were captured in a small room: their fastest are
    the nearest a game's pace."""
    near = math.radians(15.0)
    cyc = [x for x in c.cycles()
           if x[3] < 0.25 and abs(wrap(going(c, x[0], x[1]) - math.radians(aim))) < near]
    assert cyc, "the clip has no straight cycles"
    best = sorted(cyc, key=lambda x: -x[2])[:count]
    samples = []
    for a, b, _, _, _ in best:
        samples.append([in_place(c, a + (b - a) * i / SAMPLES) for i in range(SAMPLES + 1)])
    stride = sum(x[4] for x in best) / len(best)
    period = sum((x[1] - x[0]) * c.dt for x in best) / len(best)
    # The way it goes, in the frame of the way it faces (x its left, z ahead).
    wx = wz = 0.0
    for a, b, _, _, _ in best:
        d = (c.root[b][0] - c.root[a][0], 0.0, c.root[b][2] - c.root[a][2])
        d = q_rotate(q_axis((0, 1, 0), -math.degrees(c.facing((a + b) // 2))), d)
        wx += d[0]
        wz += d[2]
    n = math.hypot(wx, wz)
    return close(average(samples)), stride, period, stance(c, best), heel(c, best), (wx / n, wz / n)


def heel(c, cycles):
    """The share of its cycles the left heel stays where it came down: from the foot's contact
    until its ankle has moved 3 cm."""
    ankle = c.index("LeftAnkle")
    shares = []
    for a, b, _, _, _ in cycles:
        k = a
        p = c.at[a][ankle]
        while k < b and math.hypot(c.at[k][ankle][0] - p[0], c.at[k][ankle][2] - p[2]) < 0.03:
            k += 1
        shares.append((k - a) / (b - a))
    return sum(shares) / len(shares)


def stance(c, cycles):
    """The share of its cycles the left foot is on the ground: from its contact while its toe
    stays low and all but still."""
    toe = c.index("LeftToe")
    hs = [a[toe][1] for a in c.at]
    floor = sorted(hs)[len(hs) // 20]
    shares = []
    for a, b, _, _, _ in cycles:
        k = a
        while k < b and hs[k] < floor + 0.04 and c.toe_speed(toe, k) < 0.35:
            k += 1
        shares.append((k - a) / (b - a))
    return sum(shares) / len(shares)


def idle_loop(c):
    """The idle's loop: its stillest `IDLE_SECONDS`, `SAMPLES` samples over it."""
    span = int(IDLE_SECONDS / c.dt)
    best, best_k = 1e9, 0
    for k in range(0, len(c.root) - span, 15):
        a, b = c.root[k], c.root[k + span]
        moved = sum(math.hypot(c.root[i + 15][0] - c.root[i][0], c.root[i + 15][2] - c.root[i][2])
                    for i in range(k, k + span - 15, 15))
        turn = abs(wrap(c.path_yaw(k + span) - c.path_yaw(k)))
        if moved + turn < best:
            best, best_k = moved + turn, k
    hips = c.index("Hips")
    yaw = heading(c.rot[best_k][hips])
    unturn = q_axis((0, 1, 0), -math.degrees(yaw))
    centre = c.root[best_k]
    samples = []
    for i in range(SAMPLES + 1):
        local, root, _ = frame_at(c, best_k + span * i / SAMPLES)
        local = list(local)
        local[0] = q_norm(q_mul(unturn, local[0]))
        d = q_rotate(unturn, (root[0] - centre[0], 0.0, root[2] - centre[2]))
        samples.append((local, (d[0], root[1], d[2])))
    return close(samples), 0.0, span * c.dt, 1.0, 1.0, (0.0, 1.0)


RIG = ["Hips", "Chest", "Chest2", "Chest3", "Chest4", "Neck", "Head", "RightCollar",
       "RightShoulder", "RightElbow", "RightWrist", "LeftCollar", "LeftShoulder", "LeftElbow",
       "LeftWrist", "RightHip", "RightKnee", "RightAnkle", "RightToe", "LeftHip", "LeftKnee",
       "LeftAnkle", "LeftToe"]


def wrela_loop(name, doc, loop):
    samples, stride, period, share, heel_share, way = loop
    hips = ",\n".join(f"        vec3({h[0]:.4f}, {h[1]:.4f}, {h[2]:.4f})" for _, h in samples)
    turns = ",\n".join(f"        vec4({q[0]:.4f}, {q[1]:.4f}, {q[2]:.4f}, {q[3]:.4f})"
                       for t, _ in samples for q in t)
    return (f"/// {doc}\npub const {name} = Loop {{\n    stride: {stride:.4f},\n"
            f"    period: {period:.4f},\n    stance: {share:.4f},\n    heel: {heel_share:.4f},\n"
            f"    way: vec2({way[0]:.4f}, {way[1]:.4f}),\n"
            f"    hips: [\n{hips},\n    ],\n"
            f"    turns: [\n{turns},\n    ],\n}}\n")


def loops(out, idle, walk, run, side, back):
    clips = [Clip(p) for p in (idle, walk, run, side, back)]
    for c in clips:
        assert c.names == RIG, f"the clip's joints aren't the rig's: {c.names}"
    i = idle_loop(clips[0])
    w, r = cycle_loop(clips[1]), cycle_loop(clips[2])
    # The sidestep to the right (the left is its mirror image), the walk back straight back.
    sd, bk = cycle_loop(clips[3], aim=-80.0), cycle_loop(clips[4], aim=180.0)
    for name, (s, stride, period, share, h, way) in (("idle", i), ("walk", w), ("run", r),
                                                     ("side", sd), ("back", bk)):
        print(f"{name}: {len(s)} samples, stride {stride:.2f} m in {period:.2f} s "
              f"({stride / period if period else 0:.2f} m/s), the foot down {share:.2f} of it, "
              f"the heel {h:.2f}, going ({way[0]:.2f}, {way[1]:.2f})")
    text = f"""// The motion capture's loops (`mocap`): an idle, a walk, a run, a sidestep and a walk backward, cut from 100STYLE's Neutral
// clips (Ian Mason et al., CC BY 4.0: docs/process.md) by `tools/bvh.py loops` and written
// here. Don't edit it: cut it again. Each loop is {SAMPLES} samples of every joint's turn (x, y, z,
// w; `rig`'s joints in order) and of where the hips are (in place, in the frame of the way the
// body faces: their height from the ground); a cycle's from the left foot's contact to its next,
// the average of its fastest near-straight cycles, with the distance a cycle covers (its stride,
// m), the time it takes (s), the share of it a foot is down and its heel is, and the way it goes;
// the idle's a still stretch of {IDLE_SECONDS:.0f} s.

/// How many samples a loop has.
pub const SAMPLES: u32 = {SAMPLES}

/// A loop: its stride (m: 0 for the idle), its period (s), the share of it a foot is on the
/// ground and the share its heel is (1 for the idle), the way it goes (level, unit, in the frame
/// of the way the body faces: x its left, y ahead), and its samples.
pub struct Loop: Copy {{
    pub stride: f32,
    pub period: f32,
    pub stance: f32,
    pub heel: f32,
    pub way: vec2,
    pub hips: [vec3; {SAMPLES}],
    pub turns: [vec4; {SAMPLES * len(RIG)}],
}}

"""
    text += wrela_loop("IDLE", "Standing: Neutral_ID's stillest stretch.", i) + "\n"
    text += wrela_loop("WALK", "Walking: Neutral_FW's fastest near-straight cycles.", w) + "\n"
    text += wrela_loop("RUN", "Jogging: Neutral_FR's fastest near-straight cycles.", r) + "\n"
    text += wrela_loop("SIDE", "Sidestepping to the right: Neutral_SW's fastest near-straight cycles that way.", sd) + "\n"
    text += wrela_loop("BACK", "Walking backward: Neutral_BW's fastest near-straight cycles.", bk)
    open(out, "w").write(text)


def main():
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    if args[0] == "stats":
        for p in args[1:]:
            stats(p)
    elif args[0] == "loops" and len(args) == 7:
        loops(*args[1:])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
