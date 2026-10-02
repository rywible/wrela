#!/usr/bin/env python3
"""Tests tools/headless.sh and tools/serve.py with a fake Chrome (fake-chrome.py), so no GPU:
runs take turns on the GPU lock, a Chrome that outlives its killed run keeps the lock, a killed
run stops its Chrome and cleans up, and a result is never read half written.

Each test copies both tools into a scratch repo root, serves it on a free port, and points the
lock (WRELA_GPU_LOCK) and Chrome profiles (TMPDIR) into it, so it never touches a real run's.
Needs sh, python3 and curl. Run: python3 tools/tests/headless.py"""

import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FAKE_CHROME = os.path.join(TOOLS, "tests", "fake-chrome.py")


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


def wait_for(condition, seconds, what):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = condition()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError(f"{what} within {seconds}s")


class Headless(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="wrela-headless-test.")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        os.mkdir(os.path.join(self.root, "tools"))
        for tool in ["headless.sh", "serve.py"]:
            shutil.copy(os.path.join(TOOLS, tool), os.path.join(self.root, "tools", tool))
        self.tmp = os.path.join(self.root, "tmp")
        os.mkdir(self.tmp)
        self.log = os.path.join(self.root, "fake.log")
        self.port = free_port()
        serve = os.path.join(self.root, "tools", "serve.py")
        server = subprocess.Popen(
            [sys.executable, serve, str(self.port)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self.addCleanup(server.wait)
        self.addCleanup(server.kill)
        self.runs = []
        self.addCleanup(self.stop_runs)
        wait_for(self.serving, 10, "serve.py didn't start")

    def stop_runs(self):
        # Each run is its own process group, with its fake Chrome in it. A group that's gone,
        # or holds only zombies (macOS says EPERM), needs nothing.
        for run in self.runs:
            try:
                os.killpg(run.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
            run.wait()
            run.stdout.close()

    def serving(self):
        try:
            socket.create_connection(("127.0.0.1", self.port), timeout=1).close()
        except OSError:
            return False
        return True

    def run_page(self, page, work=0.2, done="ok"):
        """Starts headless.sh on `page`, with the fake Chrome working for `work` seconds."""
        os.makedirs(os.path.join(self.root, page), exist_ok=True)
        env = dict(
            os.environ,
            WRELA_PORT=str(self.port),
            WRELA_CHROME=FAKE_CHROME,
            WRELA_GPU_LOCK=os.path.join(self.root, "gpu.lock"),
            TMPDIR=self.tmp,
            FAKE_LOG=self.log,
            FAKE_WORK=str(work),
            FAKE_DONE=done,
        )
        script = os.path.join(self.root, "tools", "headless.sh")
        run = subprocess.Popen(
            ["sh", script, page, "#run", "60"],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            start_new_session=True,
        )
        self.runs.append(run)
        return run

    def events(self, page=None, event=None):
        try:
            with open(self.log) as f:
                lines = [json.loads(line) for line in f]
        except FileNotFoundError:
            return []
        return [
            e
            for e in lines
            if (page is None or e["page"] == page) and (event is None or e["event"] == event)
        ]

    def started(self, page):
        starts = self.events(page, "start")
        return starts[0] if starts else None

    def finish(self, run, seconds=30):
        output, _ = run.communicate(timeout=seconds)
        return run.returncode, output

    def test_runs_take_turns(self):
        pages = [f"p{i}" for i in range(4)]
        runs = [self.run_page(page) for page in pages]
        for run in runs:
            status, output = self.finish(run, 60)
            self.assertEqual(status, 0, output)
        # Each Chrome starts only after the one before it has exited.
        starts = sorted(self.events(event="start"), key=lambda e: e["t"])
        exits = {e["page"]: e["t"] for e in self.events(event="exit")}
        self.assertEqual(sorted(e["page"] for e in starts), pages)
        for before, after in zip(starts, starts[1:]):
            self.assertLess(exits[before["page"]], after["t"], f"{after} overlapped {before}")

    def test_a_chrome_that_outlives_its_run_keeps_the_lock(self):
        first = self.run_page("first", work=120)
        chrome = wait_for(lambda: self.started("first"), 10, "the first run didn't start")
        second = self.run_page("second")
        time.sleep(1)
        first.kill()  # SIGKILL: no trap runs, so its Chrome lives on
        first.wait()
        time.sleep(4)  # two of headless.sh's polls
        self.assertIsNone(self.started("second"), "the second run started beside a live Chrome")
        os.kill(chrome["pid"], signal.SIGKILL)
        killed = time.time()
        second_start = wait_for(lambda: self.started("second"), 5, "the lock wasn't released")
        self.assertGreater(second_start["t"], killed)
        status, output = self.finish(second)
        self.assertEqual(status, 0, output)

    def test_a_terminated_run_stops_its_chrome_and_cleans_up(self):
        first = self.run_page("first", work=120)
        chrome = wait_for(lambda: self.started("first"), 10, "the first run didn't start")
        waiting = self.run_page("waiting")
        second = self.run_page("second")
        time.sleep(1)
        # A run terminated while it waits for the lock exits.
        waiting.terminate()
        status, output = self.finish(waiting, 5)
        self.assertNotEqual(status, 0, output)
        # One terminated while it holds the lock stops its Chrome, then releases the lock.
        first.terminate()
        status, output = self.finish(first, 5)
        self.assertNotEqual(status, 0, output)
        self.assertFalse(alive(chrome["pid"]), "its Chrome is still running")
        self.assertFalse(os.path.exists(chrome["profile"]), "its profile is still there")
        status, output = self.finish(second)
        self.assertEqual(status, 0, output)
        (exited,) = self.events("first", "exit")
        self.assertGreater(self.started("second")["t"], exited["t"])
        self.assertIsNone(self.started("waiting"))
        profiles = [name for name in os.listdir(self.tmp) if name.startswith("wrela-chrome.")]
        self.assertEqual(profiles, [], "a Chrome profile was left behind")

    def test_a_result_is_never_read_half_written(self):
        status, output = self.finish(self.run_page("slow", done="slow"))
        self.assertEqual(status, 0, output)
        results = os.path.join(self.root, "slow", "results")
        with open(os.path.join(results, "DONE")) as f:
            self.assertEqual(f.read(), "ok")
        self.assertEqual(sorted(os.listdir(results)), ["DONE", "console.log"])


if __name__ == "__main__":
    unittest.main()
