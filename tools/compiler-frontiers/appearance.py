"""Deterministic, deliberately bounded appearance experiments. NumPy + Pillow.

These are CPU research references, not measurements of the production renderer.
Branch model: flat needle plates, centroid self-shadowing, diffuse reflection and
15% back transmission; no multiple scattering, continuous visibility or trunk.
"""
from pathlib import Path
import hashlib
import json
import math
import sys
import time
import numpy as np
from PIL import Image, ImageDraw, ImageFont

OUT = Path(sys.argv[1] if len(sys.argv) > 1 else "output/compiler-frontiers")
OUT.mkdir(parents=True, exist_ok=True)


def unit(v):
    return v / np.maximum(np.linalg.norm(v, axis=-1, keepdims=True), 1e-30)


def error(a, b):
    return dict(relativeL2=float(np.linalg.norm(a-b)/max(np.linalg.norm(a), 1e-30)),
                rms=float(np.sqrt(np.mean((a-b)**2))), maximum=float(np.max(np.abs(a-b))))


def ggx(normals, view, light, rough):
    nv, nl = normals @ view, normals @ light
    h = unit(view+light)
    nh = normals @ h
    a2 = rough**4
    q = np.sum(np.cross(normals, h)**2, axis=-1) + a2*nh*nh
    sv = np.sqrt(a2 + (1-a2)*nv*nv)
    sl = np.sqrt(a2 + (1-a2)*nl*nl)
    fresnel = .04+.96*(1-np.clip(view@h, 0, 1))**5
    response = a2*fresnel*np.maximum(nl, 0)/np.maximum(2*np.pi*q*q*(nl*sv+nv*sl), 1e-30)
    return np.where((nv > 0) & (nl > 0), response, 0)


def normal_aggregate(normals, material, divisions):
    # Preserve joint orientation/material identity; albedo is not independently averaged.
    phi = np.mod(np.arctan2(normals[:, 2], normals[:, 0]), 2*np.pi)
    band = np.minimum(divisions-1, (normals[:, 1]*divisions).astype(int))
    sector = np.minimum(2*divisions-1, (phi/(2*np.pi)*2*divisions).astype(int))
    key = material*2*divisions**2 + band*2*divisions + sector
    n, m, weights = [], [], []
    for k in np.unique(key):
        mask = key == k
        n.append(unit(normals[mask].sum(axis=0)))
        m.append(material[mask][0])
        weights.append(mask.mean())
    return np.array(n), np.array(m), np.array(weights)


def microdetail():
    rng = np.random.default_rng(519)
    n = 8192
    slopes = rng.normal(size=(n, 2)) * [.45, .26]
    slopes[:, 0] += np.where(rng.random(n) > .5, .45, -.35)
    normals = unit(np.column_stack((slopes[:, 0], np.ones(n), slopes[:, 1])))
    snow = (normals[:, 1] > .88).astype(int)
    views = unit(rng.normal(size=(160, 3)))
    lights = unit(rng.normal(size=(160, 3)))
    views[:, 1], lights[:, 1] = np.abs(views[:, 1]), np.abs(lights[:, 1])
    # Hold out all directions: clustering uses only authored surface statistics.
    def evaluate(ns, material, weights, rough):
        albedo = np.where(material == 1, .88, .09)
        return np.array([np.sum(weights*(albedo*np.maximum(ns@l, 0)/np.pi + ggx(ns, v, l, rough)))
                         for v, l in zip(views, lights)])
    results = []
    curves = {}
    for rough in [.45, .08]:
        ref = evaluate(normals, snow, np.ones(n)/n, rough)
        mean_n = unit(normals.mean(axis=0))[None, :]
        mean_albedo = np.mean(np.where(snow == 1, .88, .09))
        naive = np.array([mean_albedo*max(mean_n[0]@l, 0)/np.pi + ggx(mean_n, v, l, rough)[0]
                          for v, l in zip(views, lights)])
        rows = []
        for divisions in [2, 4, 8, 16]:
            start = time.perf_counter()
            cn, cm, cw = normal_aggregate(normals, snow, divisions)
            compile_ms = (time.perf_counter()-start)*1000
            prediction = evaluate(cn, cm, cw, rough)
            rows.append(dict(cells=len(cn), reduction=n/len(cn), bytes=len(cn)*20,
                             compileMs=compile_ms, **error(ref, prediction)))
            if divisions == 8:
                curves[str(rough)] = dict(reference=ref.tolist(), compiled=prediction.tolist(), naive=naive.tolist())
        results.append(dict(roughness=rough, referenceNormals=n, directions=len(views),
                            naive=error(ref, naive), candidates=rows))
    return dict(results=results, curves=curves)


def branch_geometry(bend=0., seed=934):
    rng = np.random.default_rng(seed)
    count = 240
    x = np.linspace(-1, 1, count) + rng.normal(0, .014, count)
    phi = np.arange(count)*2.399963 + rng.normal(0, .1, count)
    u = unit(np.column_stack((np.full(count, .8), .6*np.cos(phi), .6*np.sin(phi))))
    axis = np.tile([1., 0, 0], (count, 1))
    v = unit(np.cross(u, axis))
    centers = np.column_stack((x, .19*np.cos(phi), .19*np.sin(phi)))
    centers[:, 1] -= bend*(x+1)**2
    # Apply matching local bend to the needle frames.
    u[:, 1] -= 2*bend*(x+1)*u[:, 0]
    u = unit(u)
    v = unit(v-u*np.sum(u*v, axis=-1, keepdims=True))
    normals = unit(np.cross(u, v))
    flip = normals[:, 1] < 0
    normals[flip] *= -1
    snow = np.clip((normals[:, 1]-.5)*2, 0, 1)
    length = .20+rng.random(count)*.12
    width = .010+rng.random(count)*.007
    return centers, u, v, normals, length, width, snow


def nearest_plate(origins, direction, geometry, self_ids=None):
    c, u, v, n, length, width, _ = geometry
    denominator = n@direction
    safe = np.where(np.abs(denominator) < 1e-10, 1e-10, denominator)
    delta = c[None, :, :]-origins[:, None, :]
    distance = np.sum(delta*n[None, :, :], axis=-1)/safe
    hit = origins[:, None, :]+distance[:, :, None]*direction-c[None, :, :]
    along = np.sum(hit*u[None, :, :], axis=-1)
    across = np.sum(hit*v[None, :, :], axis=-1)
    valid = (distance > 1e-5) & (np.abs(along) <= length/2) & (np.abs(across) <= width/2) & (np.abs(denominator) > 1e-10)
    if self_ids is not None:
        valid[np.arange(len(origins)), self_ids] = False
    distance = np.where(valid, distance, np.inf)
    index = np.argmin(distance, axis=1)
    return np.where(np.isfinite(distance[np.arange(len(origins)), index]), index, -1)


def camera_ids(geometry, azimuth, size=192):
    view = unit(np.array([np.sin(azimuth)*.4, .8, np.cos(azimuth)*.8]))
    right = unit(np.cross([0., 1, 0], view))
    up = np.cross(view, right)
    yy, xx = np.mgrid[:size, :size]
    origins = ((xx.ravel()+.5)/size-.5)[:, None]*right*2.7 + (.5-(yy.ravel()+.5)/size)[:, None]*up*2.7+view*5
    # Batching bounds scratch memory.
    ids = np.concatenate([nearest_plate(part, -view, geometry) for part in np.array_split(origins, 12)])
    return ids.reshape(size, size)


def branch_response(geometry, angles, shadow=True):
    centers, _, _, normals, _, _, snow = geometry
    albedo = .08*(1-snow)+.85*snow
    result = []
    for angle in angles:
        light = unit(np.array([np.cos(angle), .55, np.sin(angle)]))
        visible = np.ones(len(centers))
        if shadow:
            visible = (nearest_plate(centers+light*1e-4, light, geometry, np.arange(len(centers))) < 0).astype(float)
        cosine = normals@light
        result.append(albedo*(np.maximum(cosine, 0) + .15*(1-snow)*np.maximum(-cosine, 0))*visible/np.pi)
    return np.array(result)


def basis(angles, frequency):
    columns = [np.ones(len(angles))]
    for k in range(1, frequency+1):
        columns.extend([np.cos(k*angles), np.sin(k*angles)])
    return np.array(columns).T


def trig_roots(coefficients):
    """All roots of A cos(theta) + B sin(theta) + C = 0 on [0,2pi)."""
    a, b, c = coefficients
    radius = math.hypot(a, b)
    if radius < 1e-13 or abs(c) > radius:
        return []
    phase = math.atan2(b, a)
    delta = math.acos(np.clip(-c/radius, -1, 1))
    return [(phase-delta) % (2*np.pi), (phase+delta) % (2*np.pi)]


def compile_shadow_events(geometry):
    """Compile exact real-arithmetic event intervals for this fixed directional-light
    orbit and plate-centroid visibility. Boundaries retain FP uncertainty; no claim
    that these intervals describe spatially varying or deforming shadows."""
    centers, u, v, normals, length, width, _ = geometry
    all_intervals = []
    for i, origin in enumerate(centers):
        intervals = []
        for j, center in enumerate(centers):
            if i == j:
                continue
            delta = origin-center
            numerator = -normals[j]@delta
            denominator = np.array([normals[j, 0], normals[j, 2], normals[j, 1]*.55])
            roots = [0., 2*np.pi, *trig_roots(denominator)]
            for axis, extent in [(u[j], length[j]/2), (v[j], width[j]/2)]:
                projection = np.array([axis[0], axis[2], axis[1]*.55])
                for sign in [-1, 1]:
                    roots.extend(trig_roots((delta@axis-sign*extent)*denominator+numerator*projection))
            roots = sorted(set(roots))
            for low, high in zip(roots[:-1], roots[1:]):
                mid = (low+high)/2
                direction = np.array([math.cos(mid), .55, math.sin(mid)])
                den = normals[j]@direction
                if abs(den) < 1e-12:
                    continue
                t = numerator/den
                hit = delta+t*direction
                if t > 1e-5 and abs(hit@u[j]) <= length[j]/2 and abs(hit@v[j]) <= width[j]/2:
                    intervals.append((low, high))
        merged = []
        for low, high in sorted(intervals):
            if merged and low <= merged[-1][1]+1e-12:
                merged[-1][1] = max(merged[-1][1], high)
            else:
                merged.append([low, high])
        all_intervals.append(np.array(merged).reshape(-1, 2))
    return all_intervals


def event_visibility(intervals, angles):
    result = np.ones((len(angles), len(intervals)))
    for i, spans in enumerate(intervals):
        for low, high in spans:
            result[(angles >= low) & (angles < high), i] = 0
    return result


def boundary_probe(geometry, intervals):
    """Independent ray/rectangle intersections close to stored f32 event endpoints."""
    packed = [spans.astype(np.float32).astype(float) for spans in intervals]
    candidates = [(owner, boundary) for owner, spans in enumerate(intervals) for boundary in spans.ravel()]
    selected = np.random.default_rng(194).choice(len(candidates), 512, replace=False)
    rays = [(candidates[i][0], (candidates[i][1]+epsilon) % (2*np.pi)) for i in selected for epsilon in [-1e-3, -5e-5, -1e-5, -1e-7, 0, 1e-7, 1e-5, 5e-5, 1e-3]]
    centers, u, v, normals, length, width, _ = geometry
    wrong, guarded, outside_guard = 0, 0, 0
    for owner, angle in rays:
        direction = unit(np.array([math.cos(angle), .55, math.sin(angle)]))
        origin = centers[owner]+direction*1e-4
        den = normals@direction
        safe = np.where(np.abs(den) < 1e-10, 1e-10, den)
        t = np.sum((centers-origin)*normals, axis=-1)/safe
        hit = origin+t[:, None]*direction-centers
        blocked = (t > 1e-5) & (np.abs(np.sum(hit*u, axis=-1)) <= length/2) & (np.abs(np.sum(hit*v, axis=-1)) <= width/2) & (np.abs(den) > 1e-10)
        blocked[owner] = False
        spans = packed[owner]
        predicted = np.any((angle >= spans[:, 0]) & (angle < spans[:, 1]))
        guard = bool(np.any(np.abs(spans-angle) < 2e-5))
        mismatch = bool(predicted != blocked.any())
        wrong += mismatch
        guarded += guard
        outside_guard += mismatch and not guard
    return dict(rays=len(rays), float32EndpointMismatches=wrong, fallbackQueries=guarded,
                mismatchesOutsideGuard=outside_guard, angularGuardRadians=2e-5,
                note="Measured guard, not a certified floating-point bound; deliberately adversarial boundary queries")


def compile_branch_integrals(geometry, intervals):
    """Piecewise antiderivatives of visibility * diffuse response on a 1D light
    orbit. This is an angular line emitter, NOT a finite solar disk integral."""
    tables = []
    for normal, snow, spans in zip(geometry[3], geometry[-1], intervals):
        raw = np.array([normal[0], normal[2], normal[1]*.55])/math.sqrt(1+.55**2)
        albedo = .08*(1-snow)+.85*snow
        knots = np.array(sorted(set([0., 2*np.pi, *spans.ravel(), *trig_roots(raw)])))
        coefficients, cumulative = [], [0.]
        for lo, hi in zip(knots[:-1], knots[1:]):
            mid = (lo+hi)/2
            visible = not np.any((mid >= spans[:, 0]) & (mid < spans[:, 1]))
            cosine = raw@np.array([math.cos(mid), math.sin(mid), 1])
            factor = (1 if cosine >= 0 else -.15*(1-snow))*albedo/np.pi if visible else 0
            coefficient = raw*factor
            coefficients.append(coefficient)
            integral = coefficient@np.array([math.sin(hi)-math.sin(lo), -math.cos(hi)+math.cos(lo), hi-lo])
            cumulative.append(cumulative[-1]+integral)
        tables.append((knots, np.array(coefficients), np.array(cumulative)))
    return tables


def integrated_branch(tables, angles, width):
    result = np.zeros((len(angles), len(tables)))
    for i, (knots, coefficients, cumulative) in enumerate(tables):
        def primitive(x):
            period = np.floor(x/(2*np.pi))
            phase = np.mod(x, 2*np.pi)
            index = np.minimum(np.searchsorted(knots, phase, side="right")-1, len(coefficients)-1)
            start = knots[index]
            local = np.column_stack((np.sin(phase)-np.sin(start), -np.cos(phase)+np.cos(start), phase-start))
            return period*cumulative[-1]+cumulative[index]+np.sum(coefficients[index]*local, axis=-1)
        result[:, i] = (primitive(angles+width/2)-primitive(angles-width/2))/width
    return result


def branch_probe():
    train_angles = np.arange(32)*2*np.pi/32
    test_angles = train_angles+np.pi/32
    geometry = branch_geometry()
    cameras = [camera_ids(geometry, a) for a in [-.5, .6, 1.9]]
    weight = np.zeros(240)
    for ids in cameras:
        weight += np.bincount(ids[ids >= 0], minlength=240)
    visible = weight > 0
    results = []
    saved = {}
    for shadows in [False, True]:
        start = time.perf_counter()
        training = branch_response(geometry, train_angles, shadows)
        reference = branch_response(geometry, test_angles, shadows)
        _, singular, vt = np.linalg.svd(training, full_matrices=False)
        compile_ms = (time.perf_counter()-start)*1000
        for rank in [4, 8, 16]:
            spatial = vt[:rank]
            train_weights = training@spatial.T
            coefficients = np.linalg.lstsq(basis(train_angles, 15), train_weights, rcond=None)[0]
            prediction = basis(test_angles, 15)@coefficients@spatial
            train_prediction = train_weights@spatial
            weighted_error = error(reference[:, visible]*np.sqrt(weight[visible]), prediction[:, visible]*np.sqrt(weight[visible]))
            results.append(dict(shadows=shadows, rank=rank, bytes=(spatial.size+coefficients.size)*4,
                                compileMs=compile_ms, training=error(training, train_prediction),
                                heldOut=weighted_error, negativeFraction=float(np.mean(prediction < 0)),
                                clamped=error(reference[:, visible]*np.sqrt(weight[visible]), np.maximum(prediction[:, visible], 0)*np.sqrt(weight[visible]))))
            if rank == 8 and shadows:
                saved = dict(reference=reference, prediction=prediction, coefficients=coefficients, spatial=spatial)
    # In-domain pose interpolation must be tested against fresh visibility, not against the fit itself.
    bend_rows = []
    for bend in [.03, .12, .3]:
        changed_geometry = branch_geometry(bend)
        actual = branch_response(changed_geometry, test_angles)
        stale = saved["prediction"]
        left = branch_response(branch_geometry(0), test_angles)
        right = branch_response(branch_geometry(.12), test_angles)
        interpolated = left+(right-left)*(bend/.12)
        bend_rows.append(dict(bend=bend, stale=error(actual, stale), poseInterpolation=error(actual, interpolated),
                              interpolationDomain=[0, .12]))
    # Follow the failed smooth fit with a structurally different representation.
    start = time.perf_counter()
    intervals = compile_shadow_events(geometry)
    event_compile_ms = (time.perf_counter()-start)*1000
    event_angles = np.random.default_rng(832).random(257)*2*np.pi
    exact = branch_response(geometry, event_angles)
    packed = [spans.astype(np.float32).astype(float) for spans in intervals]
    predicted = branch_response(geometry, event_angles, False)*event_visibility(packed, event_angles)
    np.savez_compressed(OUT/"shadow-events.npz", endpoints=np.concatenate(packed).astype(np.float32),
                        offsets=np.cumsum([0]+[len(spans) for spans in packed]).astype(np.uint32),
                        centers=geometry[0], tangent=geometry[1], bitangent=geometry[2], normals=geometry[3],
                        lengths=geometry[4], widths=geometry[5], snow=geometry[6],
                        testAngles=event_angles, reference=exact, predicted=predicted)
    event_metrics = dict(compileMs=event_compile_ms, intervals=sum(len(x) for x in intervals),
                         bytes=sum(x.size for x in intervals)*4+(len(intervals)+1)*4,
                         heldOutLights=len(event_angles), comparisons=exact.size, **error(exact, predicted))
    # Random-angle agreement does not establish numerical safety at event boundaries.
    boundaries = np.concatenate([x.ravel() for x in intervals if len(x)])
    event_metrics["boundaryCount"] = len(boundaries)
    event_metrics["fixedPoseOnly"] = True
    event_metrics["boundaryStress"] = boundary_probe(geometry, intervals)
    event_metrics["randomGuardHits"] = sum(int(np.any(np.abs(spans-angle) < 2e-5)) for spans in packed for angle in event_angles)
    start = time.perf_counter()
    integrals = compile_branch_integrals(geometry, intervals)
    integral_compile_ms = (time.perf_counter()-start)*1000
    integral_angles = np.random.default_rng(628).random(16)*2*np.pi
    width = .12
    start = time.perf_counter()
    integrated = integrated_branch(integrals, integral_angles, width)
    integral_eval_ms = (time.perf_counter()-start)*1000
    def quadrature(count):
        angles = integral_angles[:, None]+((np.arange(count)+.5)/count-.5)*width
        return branch_response(geometry, angles.ravel()).reshape(len(integral_angles), count, 240).mean(axis=1)
    reference_integral = quadrature(256)
    coarse_integral = quadrature(128)
    integral_metrics = dict(cases=len(integral_angles), needleQueries=integrated.size, angularWidth=width,
                            emitter="uniform angular line on the fixed orbit, not a solar disk",
                            compileMs=integral_compile_ms, evaluateMs=integral_eval_ms,
                            float64Bytes=sum(sum(a.nbytes for a in table) for table in integrals),
                            compiled=error(reference_integral, integrated),
                            referenceConvergence=error(reference_integral, coarse_integral),
                            centerSample=error(reference_integral, branch_response(geometry, integral_angles)))
    ids = cameras[0]
    j = int(np.argmax(np.sum((saved["reference"]-saved["prediction"])**2*weight[None, :], axis=1)))
    def image_of(values):
        image = np.zeros((*ids.shape, 3))
        image[:] = [.015, .025, .035]
        mask = ids >= 0
        image[mask] = np.maximum(values[ids[mask], None], 0)*np.array([.9, 1., .94])+.015
        return image*3
    compiled_events = branch_response(geometry, test_angles, False)*event_visibility(packed, test_angles)
    montage([(image_of(saved["reference"][j]), "Held-out reference"),
             (image_of(saved["prediction"][j]), "Rank 8 + angular fit"),
             (image_of(compiled_events[j]), "Compiled shadow events")], OUT/"branch.png")
    # A fixed camera/light-orbit animation reveals changes that a still can hide.
    frames = []
    for frame in range(len(test_angles)):
        panels = [image_of(saved["reference"][frame]), image_of(saved["prediction"][frame]), image_of(compiled_events[frame])]
        sheet = Image.new("RGB", (768, 286), "#111925")
        draw = ImageDraw.Draw(sheet)
        for column, (data, label) in enumerate(zip(panels, ["Reference", "Smooth fit (fails)", "Shadow events"])):
            img = Image.fromarray((np.clip(data, 0, 1)**(1/2.2)*255).astype(np.uint8)).resize((256, 256), Image.Resampling.NEAREST)
            sheet.paste(img, (column*256, 30))
            draw.text((column*256+10, 9), label, fill="white")
        frames.append(sheet)
    frames[0].save(OUT/"branch-orbit.gif", save_all=True, append_images=frames[1:], duration=110, loop=0)
    np.savez_compressed(OUT/"branch-data.npz", **saved, cameraIds=np.array(cameras), trainAngles=train_angles, testAngles=test_angles)
    return dict(needles=240, trainLights=32, heldOutLights=32, cameras=3, lightElevation=math.atan(.55),
                model="flat plates, per-plate centroid shadow, diffuse plus 15% back transmission; fixed light-elevation orbit; no multiple scattering",
                candidates=results, bends=bend_rows, events=event_metrics, integratedEvents=integral_metrics,
                snowNeedles=int(np.sum(geometry[-1] > 0)), stillLightIndex=j)


def montage(panels, path):
    width, height = 360, 310
    sheet = Image.new("RGB", (width*len(panels), height), "#111925")
    draw = ImageDraw.Draw(sheet)
    for i, (data, label) in enumerate(panels):
        img = Image.fromarray((np.clip(data, 0, 1)**(1/2.2)*255).astype(np.uint8))
        scale = min((width-24)/img.width, (height-54)/img.height)
        img = img.resize((round(img.width*scale), round(img.height*scale)), Image.Resampling.NEAREST)
        sheet.paste(img, (i*width+(width-img.width)//2, 38))
        draw.text((i*width+12, 12), label, fill="white")
    sheet.save(path)


def weather_probe(warming=2.4, save=True):
    # A transparent mass-accounting toy, not a validated snow/thermal simulator.
    size = 96
    y, x = np.mgrid[:size, :size]/size
    ground = .6*x + .15*np.sin(y*8)+.12*np.cos(x*7+y*3)
    canopy = np.exp(-((x-.42)**2+(y-.55)**2)/.022)
    exposure = 1-.82*canopy
    snow = .035*exposure
    # One conservative directional drift pass with closed boundaries.
    flux = snow[:, :-1]*.12
    snow[:, :-1] -= flux
    snow[:, 1:] += flux
    initial = float(snow.sum())
    liquid, ice = np.zeros_like(snow), np.zeros_like(snow)
    temperature = warming + 1.8*x - 1.2*canopy
    melt = np.minimum(snow, np.maximum(temperature, 0)*.003)
    snow -= melt
    liquid += melt
    # Downhill routing on a DAG; each cell retains a 0.4 mm water film.
    order = np.argsort(ground.ravel())[::-1]
    exported = 0.
    routed = np.zeros_like(snow)
    for index in order:
        row, col = divmod(int(index), size)
        flow = max(0., liquid[row, col]-.0004)
        candidates = [(ground[r, c], r, c) for r, c in [(row-1, col), (row+1, col), (row, col-1), (row, col+1)]
                      if 0 <= r < size and 0 <= c < size and ground[r, c] < ground[row, col]]
        if candidates:
            _, r, c = min(candidates)
            liquid[row, col] -= flow
            liquid[r, c] += flow
            routed[row, col] += flow
        elif row in [0, size-1] or col in [0, size-1]:
            liquid[row, col] -= flow
            exported += flow
    before_freeze = liquid.copy()
    freeze = liquid*.8
    ice += freeze
    liquid -= freeze
    total = float(snow.sum()+liquid.sum()+ice.sum()+exported)
    # All derived appearance channels use the same state.
    coverage = 1-np.exp(-snow/.006)
    wetness = np.clip(liquid/.0004, 0, 1)
    albedo = (.13*(1-coverage)+.85*coverage)*(1-.25*wetness*(1-coverage))
    roughness = coverage*.85 + (1-coverage)*np.clip(.75-.65*np.clip(ice/.003, 0, 1)-.2*wetness, .08, .9)
    def color(field, color):
        return np.clip(field[..., None], 0, 1)*np.array(color)
    if save:
        montage([(color(snow/.035, [.82, .9, 1]), "Snow after warming"),
                 (color(np.log1p(routed*100)/4, [.12, .65, 1]), "Routed meltwater (log)"),
                 (color(ice/.003, [.4, .85, 1]), "Refrozen water"),
                 (color(albedo, [1, 1, 1]), "Derived albedo")], OUT/"weather.png")
        np.savez_compressed(OUT/"weather-data.npz", snow=snow, liquid=liquid, ice=ice, routed=routed, ground=ground, albedo=albedo, roughness=roughness)
    # Cantilever load response: a shared snow mass controls an illustrative branch sag.
    load_kg_per_m = float(snow.mean())*1000*.025
    sag_m = load_kg_per_m*9.81*1.2**4/(8*12.)
    return dict(cells=size*size, warming=warming, model="one snowfall/drift/warming/routing/refreeze sequence; equal unit cell area; water-equivalent metres; illustrative coefficients",
                initialWaterEquivalent=initial, finalSnow=float(snow.sum()), finalLiquid=float(liquid.sum()),
                finalIce=float(ice.sum()), exported=exported, conservationAbsolute=abs(total-initial),
                minimumState=float(min(snow.min(), liquid.min(), ice.min())), refreezeError=float(np.max(np.abs(before_freeze-liquid-ice))),
                derivedChannels=["snow coverage", "albedo", "roughness", "wetness", "ice", "branch load"],
                illustrativeSagMetres=sag_m, independentArtistControlsRequired=True)


if __name__ == "__main__":
    started = time.perf_counter()
    result = {}
    for name, function in [("microdetail", microdetail), ("branch", branch_probe), ("weather", weather_probe)]:
        result[name] = function()
        print(name, "finished", round(time.perf_counter()-started, 2), "seconds", flush=True)
    result["weatherControls"] = [weather_probe(warming, False) for warming in [-5., 10.]]
    result["sourceSha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    result["numpyVersion"] = np.__version__
    result["elapsedSeconds"] = time.perf_counter()-started
    (OUT/"appearance.json").write_text(json.dumps(result, indent=2))
    print(json.dumps({k: v for k, v in result.items() if k != "microdetail"}, indent=2))
