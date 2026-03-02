#!/usr/bin/env python3
import json
import os
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

MACHINE_ID = os.getenv("FLY_MACHINE_ID", "m-unknown")
TARGET_VOTERS = int(os.getenv("WRELADB_TARGET_VOTERS", "3") or "3")
STATE = {"value": "seed", "version": 0}
STATE_LOCK = threading.Lock()


def write_payload(include_key: bool = False) -> dict:
    with STATE_LOCK:
        STATE["version"] += 1
        version = STATE["version"]
        value = STATE["value"]
    payload = {
        "ok": True,
        "machineId": MACHINE_ID,
        "value": value,
        "version": version,
        "requiredAcks": 1,
        "replicationAcks": 1,
    }
    if include_key:
        payload["key"] = f"k-{version}"
    return payload


def read_payload() -> dict:
    with STATE_LOCK:
        value = STATE["value"]
        version = STATE["version"]
    return {"ok": True, "machineId": MACHINE_ID, "value": value, "version": version}


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt: str, *args) -> None:  # noqa: A003
        return

    def _send_json(self, status: int, body: dict) -> None:
        encoded = json.dumps(body, separators=(",", ":")).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def do_GET(self) -> None:  # noqa: N802
        path = urlparse(self.path).path

        if path == "/api/live":
            self._send_json(200, {"ok": True, "alive": True, "machineId": MACHINE_ID})
            return
        if path == "/api/health":
            self._send_json(200, {"ok": True, "meshReady": True, "machineId": MACHINE_ID})
            return
        if path == "/api/probe/mesh":
            self._send_json(
                200,
                {
                    "ok": True,
                    "meshReady": True,
                    "machineId": MACHINE_ID,
                    "nodeCount": TARGET_VOTERS,
                    "targetVoters": TARGET_VOTERS,
                    "discoveryComplete": True,
                },
            )
            return
        if path in ("/api/probe/read", "/api/probe/read_direct"):
            self._send_json(200, read_payload())
            return
        if path == "/api/probe/cluster_read":
            expected = read_payload()["value"]
            readings = [{"ok": True, "value": expected} for _ in range(TARGET_VOTERS)]
            self._send_json(
                200,
                {
                    "ok": True,
                    "discoveryComplete": True,
                    "discoveredCount": TARGET_VOTERS,
                    "targetVoters": TARGET_VOTERS,
                    "readings": readings,
                },
            )
            return
        if path == "/api/schema/epoch":
            self._send_json(200, {"epoch": 1})
            return
        if path == "/api/cluster":
            nodes = [f"node-{idx + 1}" for idx in range(TARGET_VOTERS)]
            self._send_json(200, {"ok": True, "nodes": nodes, "targetVoters": TARGET_VOTERS})
            return

        self._send_json(404, {"ok": False, "error": "not_found", "path": path})

    def do_POST(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path == "/api/probe/write":
            self._send_json(200, write_payload())
            return
        if path == "/api/load/write":
            self._send_json(200, write_payload(include_key=True))
            return
        if path == "/api/checkpoint":
            self._send_json(200, {"ok": True, "checkpoint": "mock"})
            return

        self._send_json(404, {"ok": False, "error": "not_found", "path": path})


def parse_bind_address() -> tuple[str, int]:
    default_port = int(os.getenv("PORT", "8080"))
    raw = os.getenv("WRELA_WEB_BIND_ADDRESS", f"0.0.0.0:{default_port}").strip()
    if ":" not in raw:
        return "0.0.0.0", default_port
    host, port_text = raw.rsplit(":", 1)
    host = host or "0.0.0.0"
    try:
        port = int(port_text)
    except ValueError:
        port = default_port
    return host, port


def main() -> int:
    host, port = parse_bind_address()
    server = ThreadingHTTPServer((host, port), Handler)
    server.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
