#!/usr/bin/env python3
"""A stand-in for Chrome, for tools/tests/headless.py. It takes Chrome's command line, "works"
for FAKE_WORK seconds, then PUTs `ok` to the page's results/DONE and stays open until it's
killed, as Chrome does (for 5 minutes at most). With FAKE_DONE=slow it sends the PUT's headers,
waits 2.5 s (longer than one of headless.sh's polls), then sends the body.

It appends one JSON line per event to FAKE_LOG: `start`, `end` (its work is done), and `exit`
(on SIGTERM). It is one process, so killing it closes everything it inherited, as killing Chrome
does."""

import json
import os
import signal
import socket
import sys
import time
import urllib.parse
import urllib.request

URL = sys.argv[-1]
PROFILE = next(a.split("=", 1)[1] for a in sys.argv if a.startswith("--user-data-dir="))
PAGE = urllib.parse.urlsplit(URL).path.strip("/")


def log(event):
    line = {"event": event, "page": PAGE, "pid": os.getpid(), "profile": PROFILE, "t": time.time()}
    with open(os.environ["FAKE_LOG"], "a") as f:
        f.write(json.dumps(line) + "\n")


def on_term(_signum, _frame):
    log("exit")
    os._exit(0)


def put_done(body):
    url = urllib.parse.urlsplit(URL)
    path = f"/{PAGE}/results/DONE"
    if os.environ.get("FAKE_DONE") != "slow":
        request = urllib.request.Request(f"http://{url.netloc}{path}", data=body, method="PUT")
        urllib.request.urlopen(request).close()
        return
    with socket.create_connection((url.hostname, url.port)) as s:
        head = f"PUT {path} HTTP/1.1\r\nHost: {url.netloc}\r\nContent-Length: {len(body)}\r\n\r\n"
        s.sendall(head.encode())
        time.sleep(2.5)
        s.sendall(body)
        s.recv(4096)


signal.signal(signal.SIGTERM, on_term)
log("start")
time.sleep(float(os.environ.get("FAKE_WORK", "0.2")))
log("end")
put_done(b"ok")
time.sleep(300)  # until it's killed, but not forever if the test that started it died
