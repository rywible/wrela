#!/usr/bin/env python3
"""lens.py: the spike 09 lens as a command-line tool for agents.

It drives the lens page in headless Chrome through spikes/headless.sh (which holds the GPU lock), so
the operations run on the real GPU, exactly as the page's UI runs them. Commands are JSON; see
`lens.py help` or spikes/09-lens/README.md.

  lens.py run JOB.json [--write] [--timeout S]
      Run a command file and print the outputs as JSON. JOB.json:
        {"source": "path/to/field.wgsl", "commands": [{"op": "query", ...}, ...]}
      Paths are relative to the job file. With --write, a "write" command saves the new source over
      the job's source file (otherwise it's only printed).

  lens.py start SOURCE.wgsl [--session NAME]   start a live page on SOURCE (holds the GPU lock)
  lens.py call NAME '{"op": "query", ...}'     send one command to it, print the result
  lens.py stop NAME                            end the session (releases the GPU lock)

Images come back as paths under spikes/09-lens/results/.
"""

import json
import os
import shutil
import subprocess
import sys
import time
import uuid

HERE = os.path.dirname(os.path.abspath(__file__))
SPIKES = os.path.dirname(HERE)
JOBS = os.path.join(HERE, "jobs")
RESULTS = os.path.join(HERE, "results")
HEADLESS = os.path.join(SPIKES, "headless.sh")
ROOT = os.path.dirname(SPIKES)


def die(msg, code=2):
    print(msg, file=sys.stderr)
    sys.exit(code)


def stage(job_dir, path, base, name):
    """Copy a file the page must fetch into the job directory; return its name there."""
    src = path if os.path.isabs(path) else os.path.join(base, path)
    if not os.path.exists(src):
        die(f"lens.py: no such file: {src}")
    shutil.copyfile(src, os.path.join(job_dir, name))
    return name


def absolutize(out):
    """Turn spikes/09-lens/... paths in the outputs into absolute paths."""
    if isinstance(out, dict):
        return {k: absolutize(v) for k, v in out.items()}
    if isinstance(out, list):
        return [absolutize(v) for v in out]
    if isinstance(out, str) and out.startswith("spikes/09-lens/"):
        return os.path.join(ROOT, out)
    return out


def cmd_run(args):
    if not args:
        die(__doc__)
    job_path = args[0]
    write = "--write" in args
    timeout = 300
    if "--timeout" in args:
        timeout = int(args[args.index("--timeout") + 1])
    with open(job_path) as f:
        job = json.load(f)
    base = os.path.dirname(os.path.abspath(job_path))
    jid = time.strftime("%Y%m%d-%H%M%S-") + uuid.uuid4().hex[:6]
    job_dir = os.path.join(JOBS, jid)
    os.makedirs(job_dir)
    page_job = {"commands": []}
    if job.get("source"):
        page_job["source"] = stage(job_dir, job["source"], base, "source.wgsl")
        page_job["name"] = os.path.basename(job["source"])
    for i, c in enumerate(job.get("commands", [])):
        c = dict(c)
        if c.get("op") == "load":
            c["source"] = stage(job_dir, c["source"], base, f"load-{i}.wgsl")
        if c.get("op") == "fit" and isinstance(c.get("target"), dict):
            t = dict(c["target"])
            if "png" in t:
                t["png"] = stage(job_dir, t["png"], base, f"target-{i}.png")
            if "source" in t:
                t["source"] = stage(job_dir, t["source"], base, f"target-{i}.wgsl")
            c["target"] = t
        page_job["commands"].append(c)
    with open(os.path.join(job_dir, "job.json"), "w") as f:
        json.dump(page_job, f)
    out_path = os.path.join(RESULTS, f"api-{jid}.json")
    t0 = time.time()
    proc = subprocess.run([HEADLESS, "09-lens", f"#api={jid}", str(timeout)], capture_output=True, text=True)
    if not os.path.exists(out_path):
        die(f"lens.py: the page produced no output ({proc.stderr.strip()}); see {RESULTS}/console.log", 1)
    with open(out_path) as f:
        res = json.load(f)
    res["wallSeconds"] = round(time.time() - t0, 2)
    if write and job.get("source"):
        for o in res["outputs"]:
            if o.get("op") == "write" and "source" in o:
                dst = job["source"] if os.path.isabs(job["source"]) else os.path.join(base, job["source"])
                with open(dst, "w") as f:
                    f.write(o["source"])
                o["savedTo"] = dst
    for o in res["outputs"]:
        o.pop("source", None) if o.get("op") == "write" and not write else None
    print(json.dumps(absolutize(res), indent=1))


def session_dir(name):
    return os.path.join(JOBS, f"session-{name}")


def cmd_start(args):
    if not args:
        die(__doc__)
    src = args[0]
    name = args[args.index("--session") + 1] if "--session" in args else uuid.uuid4().hex[:6]
    d = session_dir(name)
    if os.path.exists(d):
        shutil.rmtree(d)
    os.makedirs(d)
    shutil.copyfile(src, os.path.join(d, "source.wgsl"))
    with open(os.path.join(d, "session.json"), "w") as f:
        json.dump({"source": "source.wgsl", "name": os.path.basename(src), "path": os.path.abspath(src)}, f)
    with open(os.path.join(d, "queue.json"), "w") as f:
        json.dump({"seq": 0, "cmd": {"op": "noop"}}, f)
    for p in os.listdir(RESULTS):
        if p.startswith(f"serve-session-{name}-") or p.startswith(f"serve-{name}-"):
            os.remove(os.path.join(RESULTS, p))
    log = open(os.path.join(d, "headless.log"), "w")
    subprocess.Popen([HEADLESS, "09-lens", f"#serve=session-{name}", "1800"], stdout=log, stderr=log, start_new_session=True)
    ready = os.path.join(RESULTS, f"serve-session-{name}-0.json")
    t0 = time.time()
    while not os.path.exists(ready):
        if time.time() - t0 > 3600:
            die("lens.py: the session didn't start (waiting for the GPU lock?)", 1)
        time.sleep(0.2)
    time.sleep(0.05)
    with open(ready) as f:
        info = json.load(f)
    print(json.dumps({"session": name, "startedSeconds": round(time.time() - t0, 2), "loaded": info.get("loaded"), "literals": info.get("literals"), "parts": [p["name"] for p in info.get("parts", [])]}, indent=1))


def cmd_call(args, cmd=None):
    if len(args) < 1 + (cmd is None):
        die(__doc__)
    name = args[0]
    cmd = cmd or json.loads(args[1])
    d = session_dir(name)
    if not os.path.exists(d):
        die(f"lens.py: no session {name}")
    with open(os.path.join(d, "queue.json")) as f:
        seq = json.load(f)["seq"] + 1
    tmp = os.path.join(d, "queue.tmp")
    with open(tmp, "w") as f:
        json.dump({"seq": seq, "cmd": cmd}, f)
    t0 = time.time()
    os.replace(tmp, os.path.join(d, "queue.json"))
    out = os.path.join(RESULTS, f"serve-session-{name}-{seq}.json")
    while not os.path.exists(out):
        if time.time() - t0 > 600:
            die("lens.py: no answer in 600 s", 1)
        time.sleep(0.005)
    time.sleep(0.01)
    with open(out) as f:
        res = json.load(f)
    res["roundTripMs"] = round((time.time() - t0) * 1000, 1)
    if cmd.get("op") == "write" and "--write" in args:
        with open(os.path.join(d, "session.json")) as f:
            path = json.load(f)["path"]
        with open(path, "w") as f:
            f.write(res["source"])
        res["savedTo"] = path
    print(json.dumps(absolutize(res), indent=1))


def cmd_stop(args):
    if not args:
        die(__doc__)
    cmd_call(args[:1], {"op": "quit"})


def main():
    if len(sys.argv) < 2 or sys.argv[1] in ("-h", "--help", "help"):
        print(__doc__)
        return
    op, args = sys.argv[1], sys.argv[2:]
    {"run": cmd_run, "start": cmd_start, "call": cmd_call, "stop": cmd_stop}.get(op, lambda a: die(__doc__))(args)


if __name__ == "__main__":
    main()
