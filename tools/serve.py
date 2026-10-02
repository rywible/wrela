"""Static server for the repository, plus PUT into any `results/` directory so a page can save
its own JSON and screenshots. Local use only: binds 127.0.0.1. Pages are served at their path
relative to the repo root.

Every response is cross-origin isolated (COOP same-origin, COEP require-corp, CORP same-origin),
as a game's origin will be. The S1 runtime has one render worker and no shared memory; the
headers are sent ahead of need because browsers allow a WASM memory shared between workers,
which the plan has the runtime use later, only in an isolated page."""

import http.server
import os
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # the repo root


class Handler(http.server.SimpleHTTPRequestHandler):
    # WebAssembly.instantiateStreaming needs application/wasm; module scripts need a JS type.
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
        ".mjs": "text/javascript",
        ".wgsl": "text/plain; charset=utf-8",
        ".json": "application/json",
    }

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=ROOT, **kwargs)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cross-Origin-Resource-Policy", "same-origin")
        super().end_headers()

    def do_PUT(self):
        rel = os.path.normpath(self.path.split("?", 1)[0].lstrip("/"))
        parts = rel.split(os.sep)
        if rel.startswith("..") or len(parts) < 3 or parts[-2] != "results":
            self.send_error(403, "PUT is only allowed into a results/ directory")
            return
        dest = os.path.join(ROOT, rel)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length)
        if len(body) != length:
            self.send_error(400, "the body is shorter than its Content-Length")
            return
        # Written aside, then renamed into place: a reader (headless.sh polling for DONE) sees
        # the old file or the whole new one, never a part.
        fd, part = tempfile.mkstemp(dir=os.path.dirname(dest), prefix=".", suffix=".part")
        try:
            with os.fdopen(fd, "wb") as f:
                f.write(body)
            os.replace(part, dest)
        except BaseException:
            os.unlink(part)
            raise
        self.send_response(201)
        self.end_headers()


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8417
    http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
