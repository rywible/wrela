"""Headless third-party feasibility probe, NOT a Wrela gameplay/quality test.

Run from the repository with the isolated environment described in the report.
Creates a synthetic quadruped, exercises native MuJoCo stepping and batched
open-loop rollouts. No renderer, planner, learned policy or Wrela assets run.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

import mujoco
from mujoco import rollout
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'Tools/Testing'))
from runtime import machine, performance_lease


def model_source():
    legs, motors = [], []
    for i, (x, y) in enumerate([(0.19, 0.14), (0.19, -0.14), (-0.19, 0.14), (-0.19, -0.14)]):
        legs.append(f'''<body name="hip{i}" pos="{x} {y} 0">
          <joint name="abduct{i}" axis="1 0 0" range="-0.6 0.6"/>
          <geom type="sphere" size="0.035" mass="0.08"/>
          <body name="upper{i}">
            <joint name="hipflex{i}" axis="0 1 0" range="-1.3 1.3"/>
            <geom type="capsule" fromto="0 0 0 0 0 -0.18" size="0.025" mass="0.12"/>
            <body name="lower{i}" pos="0 0 -0.18">
              <joint name="knee{i}" axis="0 1 0" range="-1.8 1.8"/>
              <geom type="capsule" fromto="0 0 0 0 0 -0.18" size="0.02" mass="0.08"/>
              <geom name="foot{i}" type="sphere" pos="0 0 -0.18" size="0.035" mass="0.04"/>
            </body>
          </body>
        </body>''')
        for name in [f'abduct{i}', f'hipflex{i}', f'knee{i}']:
            motors.append(f'<position joint="{name}" kp="30" kv="1.5" ctrlrange="-0.5 0.5" forcerange="-4 4"/>')
    return f'''<mujoco model="synthetic-local-feasibility">
      <compiler angle="radian"/>
      <option timestep="0.004166666666666667" integrator="implicitfast" solver="Newton" iterations="20" gravity="0 0 -9.81"/>
      <default><joint damping="0.1" armature="0.003"/><geom friction="0.8 0.005 0.0001" condim="3"/></default>
      <worldbody><geom name="ground" type="plane" size="3 3 0.1"/>
        <body name="torso" pos="0 0 0.41"><freejoint/>
          <geom type="box" size="0.22 0.12 0.07" mass="2"/>
          {''.join(legs)}
        </body>
      </worldbody><actuator>{''.join(motors)}</actuator>
    </mujoco>'''


def stats(values):
    v = np.asarray(values)
    return dict(samples=len(v), median_ms=float(np.median(v)), p95_ms=float(np.percentile(v, 95)),
                p99_ms=float(np.percentile(v, 99)), max_ms=float(v.max()), raw_ms=v.tolist())


def state(model, data, spec):
    out = np.empty(mujoco.mj_stateSize(model, spec))
    mujoco.mj_getState(model, data, out, spec)
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True, help='A new directory; never overwrites a report')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    xml = model_source()
    model = mujoco.MjModel.from_xml_string(xml)
    data = mujoco.MjData(model)
    rng = np.random.default_rng(813)
    result = dict(machine=machine(), python=platform.python_version(), mujoco=mujoco.__version__,
                  numpy=np.__version__, timestamp=time.strftime('%Y-%m-%dT%H:%M:%S%z'),
                  logical_cpus=os.cpu_count(), load_before=os.getloadavg(),
                  probe_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  model_sha256=hashlib.sha256(xml.encode()).hexdigest(),
                  model=dict(nq=model.nq, nv=model.nv, actuators=model.nu, bodies_excluding_world=model.nbody-1,
                             geoms=model.ngeom, timestep=model.opt.timestep, solver_iterations=model.opt.iterations),
                  limitations=['Synthetic articulated proxy; not Frostling or Vesper.',
                               'No Wrela renderer, wind, skinning, game logic, task objective or trained controller.',
                               'Rollout timings exclude candidate generation, scoring and action selection.',
                               'Short CPU probe; foreground load and thermal behavior are uncontrolled.',
                               'No rendered inspection or artistic-quality conclusion.'])
    processes = subprocess.check_output(['ps', '-axo', 'comm='], text=True)
    result['wrela_render_processes'] = [p.strip() for p in processes.splitlines()
        if any('/Contents/MacOS/' + n in p for n in ['Soundstage', 'Sanctuary', 'Cave'])]
    result['power_before'] = subprocess.run(['pmset', '-g', 'therm'], capture_output=True, text=True).stdout.strip()
    started = time.perf_counter()
    with performance_lease():
        for _ in range(240):
            mujoco.mj_step(model, data)
        timings, contacts = [], []
        for frame in range(600):
            begin = time.perf_counter_ns()
            # A deterministic varying motor command, not goal-directed locomotion.
            data.ctrl[:] = 0.12 * np.sin(data.time * 3 + np.arange(model.nu) * 0.4)
            if frame in [120, 360]:
                data.xfrc_applied[1, 1] = 6
            else:
                data.xfrc_applied[1, 1] = 0
            mujoco.mj_step(model, data, nstep=4)
            timings.append((time.perf_counter_ns() - begin) / 1e6)
            contacts.append(data.ncon)
        result['four_substep_frame'] = stats(timings)
        result['contact_count'] = dict(min=min(contacts), max=max(contacts), mean=float(np.mean(contacts)))
        result['simulation_warnings'] = data.warning.number.tolist()
        assert np.isfinite(data.qpos).all() and not np.any(data.warning.number)

        spec = mujoco.mjtState.mjSTATE_INTEGRATION
        checkpoint = state(model, data, spec)
        controls = rng.uniform(-0.15, 0.15, (120, model.nu))
        trajectories = []
        for _ in range(2):
            restored = mujoco.MjData(model)
            mujoco.mj_setState(model, restored, checkpoint, spec)
            trajectory = []
            for command in controls:
                restored.ctrl[:] = command
                mujoco.mj_step(model, restored)
                trajectory.append(state(model, restored, spec))
            trajectories.append(np.asarray(trajectory))
        result['same_process_checkpoint_replay_exact'] = bool(np.array_equal(*trajectories))
        assert result['same_process_checkpoint_replay_exact']

        # Start every hypothetical trajectory from the same contact-rich state.
        initial = state(model, data, mujoco.mjtState.mjSTATE_FULLPHYSICS)
        result['rollouts'] = []
        for batch, steps, threads in [(8, 60, 1), (8, 60, 4), (32, 120, 1), (32, 120, 4), (64, 120, 4)]:
            work = [mujoco.MjData(model) for _ in range(threads)]
            # Smooth random candidate controls with 4 knots.
            knots = rng.uniform(-0.25, 0.25, (batch, 4, model.nu))
            commands = np.empty((batch, steps, model.nu))
            for b in range(batch):
                for j in range(model.nu):
                    commands[b, :, j] = np.interp(np.linspace(0, 3, steps), np.arange(4), knots[b, :, j])
            states = np.empty((batch, steps, len(initial)))
            sensors = np.empty((batch, steps, model.nsensordata))
            times = []
            with rollout.Rollout(nthread=threads) as pool:
                for repetition in range(23):
                    begin = time.perf_counter_ns()
                    pool.rollout(model, work, initial, commands, state=states, sensordata=sensors)
                    elapsed = (time.perf_counter_ns() - begin) / 1e6
                    if repetition >= 3:
                        times.append(elapsed)
            expected_times = initial[0] + np.arange(1, steps + 1) * model.opt.timestep
            assert np.isfinite(states).all() and np.allclose(states[:, :, 0], expected_times, atol=1e-10, rtol=0)
            result['rollouts'].append(dict(candidates=batch, horizon_seconds=steps*model.opt.timestep,
                                           physics_steps=steps, worker_threads=threads, **stats(times)))
    result['elapsed_wall_seconds'] = time.perf_counter() - started
    result['load_after'] = os.getloadavg()
    result['power_after'] = subprocess.run(['pmset', '-g', 'therm'], capture_output=True, text=True).stdout.strip()
    (args.output / 'model.xml').write_text(xml)
    (args.output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
    concise = {k: v for k, v in result.items() if k not in ['rollouts', 'four_substep_frame']}
    concise['four_substep_frame'] = {k:v for k,v in result['four_substep_frame'].items() if k != 'raw_ms'}
    concise['rollouts'] = [{k:v for k,v in r.items() if k != 'raw_ms'} for r in result['rollouts']]
    print(json.dumps(concise, indent=2))


if __name__ == '__main__':
    main()
