#!/bin/sh
# Runs a page in headless Chrome (a throwaway profile) and waits for it to finish. The page
# signals completion by PUTting results/DONE: `ok` for a run that succeeded, anything else (the
# runtime writes `failed`, with the error in results/run.json) for one that didn't. Console
# output from the page goes to <page-dir>/results/console.log, so WGSL compile errors and
# exceptions are visible. Exits 0 only for `ok`.
#
# Usage: tools/headless.sh <page-dir relative to the repo root> [hash] [timeout-seconds]
#   tools/headless.sh examples/first-light/out '#run' 120   (after `wrela build examples/first-light`)
#
# Needs tools/serve.py on 127.0.0.1:$WRELA_PORT (default 8417): python3 tools/serve.py 8417.
# WRELA_CHROME is the Chrome binary (default: Google Chrome's on macOS, else google-chrome,
# chromium or chromium-browser from PATH); WRELA_CHROME_FLAGS adds flags to its command line.
# Only one run uses the GPU at a time (see the lock below); others wait their turn.
# tools/tests/headless.py tests this script with a fake Chrome.

set -u
PAGE="${1:?usage: headless.sh <page-dir> [hash] [timeout-seconds]}"
HASH="${2:-#run}"
TIMEOUT="${3:-600}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"   # the repo root
RESULTS="$ROOT/$PAGE/results"
PORT="${WRELA_PORT:-8417}"
case "$PORT" in ''|*[!0-9]*) echo "WRELA_PORT must be a port number, not '$PORT'" >&2; exit 2;; esac

CHROME="${WRELA_CHROME:-}"
if [ -z "$CHROME" ]; then
  MAC="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
  if [ -x "$MAC" ]; then
    CHROME="$MAC"
  else
    for name in google-chrome chromium chromium-browser; do
      if command -v "$name" >/dev/null 2>&1; then CHROME="$(command -v "$name")"; break; fi
    done
  fi
fi
[ -n "$CHROME" ] || { echo "no Chrome found; set WRELA_CHROME to its binary" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "headless.sh needs python3, for its GPU lock" >&2; exit 2; }

curl -s -o /dev/null "http://127.0.0.1:$PORT/" || { echo "the server isn't running on $PORT (python3 tools/serve.py $PORT)" >&2; exit 2; }

# Every way out of this script stops Chrome and removes its profile. INT and TERM exit, which
# runs the EXIT trap; a trap that only cleaned up would let the script carry on with Chrome
# running.
PID=
PROFILE="$(mktemp -d "${TMPDIR:-/tmp}/wrela-chrome.XXXXXX")"
stop_chrome() {
  if [ -n "$PID" ] && kill "$PID" 2>/dev/null; then
    sleep 1
    kill -9 "$PID" 2>/dev/null
  fi
  PID=
}
trap 'stop_chrome; rm -rf "$PROFILE"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# One GPU user at a time. On 2026-10-01, ~12 concurrent headless runs starved WindowServer of GPU
# time for over 5 s; macOS's watchdog killed it and the whole desktop reset. Runs now queue on an
# flock(2) of one file, held through fd 9. The kernel releases it when the last process holding
# that descriptor exits, however it dies, so there's no dead holder's lock to reclaim (which
# can't be done without a race), and Chrome, which inherits fd 9, keeps the GPU locked for as long
# as it outlives this script. sh has no flock and macOS has no flock(1), so python3 takes it. A
# lock on a file that's since been replaced at the path doesn't count. The file's text names the
# holder, for a waiter's message. Older copies of this script make the same path a directory:
# this waits while one is there, and they wait while the file is.
LOCK="${WRELA_GPU_LOCK:-/tmp/wrela-gpu.lock}"   # tools/tests/headless.py moves it
take_lock() {
  { true >>"$LOCK"; } 2>/dev/null
  command exec 9<"$LOCK" || return 1
  python3 - "$LOCK" <<'EOF'
import fcntl, os, stat, sys
held = os.fstat(9)
if not stat.S_ISREG(held.st_mode):
    sys.exit(1)  # an older headless.sh's lock directory
try:
    fcntl.flock(9, fcntl.LOCK_EX | fcntl.LOCK_NB)
except BlockingIOError:
    sys.exit(1)
now = os.stat(sys.argv[1])
sys.exit(0 if (now.st_dev, now.st_ino) == (held.st_dev, held.st_ino) else 1)
EOF
}
holder() {
  if [ -d "$LOCK" ]; then
    echo "pid $(cat "$LOCK/pid" 2>/dev/null), an older headless.sh: $(cat "$LOCK/page" 2>/dev/null)"
  else
    cat "$LOCK" 2>/dev/null
  fi
}
WAITED=0
until take_lock; do
  [ "$WAITED" -eq 0 ] && echo "waiting for the GPU lock (held by $(holder))" >&2
  WAITED=$((WAITED + 2))
  [ "$WAITED" -ge 3600 ] && { echo "gave up waiting for the GPU lock after an hour" >&2; exit 3; }
  sleep 2
done
{ echo "pid $$: $PAGE $HASH" > "$LOCK"; } 2>/dev/null
[ "$WAITED" -gt 0 ] && echo "got the GPU lock after ${WAITED}s" >&2

mkdir -p "$RESULTS"
rm -f "$RESULTS/DONE" "$RESULTS/run.json" "$RESULTS/frame.png"

# WRELA_CHROME_FLAGS is split into words on purpose.
# shellcheck disable=SC2086
"$CHROME" --headless=new --user-data-dir="$PROFILE" --no-first-run --no-default-browser-check \
  --disable-extensions --disable-background-timer-throttling --disable-renderer-backgrounding \
  --enable-logging=stderr --v=0 --window-size=1920,1080 ${WRELA_CHROME_FLAGS:-} \
  "http://127.0.0.1:$PORT/$PAGE/$HASH" 2> "$RESULTS/chrome.log" &
PID=$!
{ echo "pid $$ (chrome $PID): $PAGE $HASH" > "$LOCK"; } 2>/dev/null

START=$(date +%s)
STATUS=1
while :; do
  if [ -f "$RESULTS/DONE" ]; then
    if [ "$(cat "$RESULTS/DONE")" = ok ]; then
      STATUS=0
    else
      echo "the page reported a failed run: $(grep '"error"' "$RESULTS/run.json" 2>/dev/null)" >&2
    fi
    break
  fi
  if ! kill -0 "$PID" 2>/dev/null; then echo "chrome exited early" >&2; break; fi
  if [ $(( $(date +%s) - START )) -ge "$TIMEOUT" ]; then echo "timed out after ${TIMEOUT}s" >&2; break; fi
  sleep 1
done
stop_chrome

# Page console lines only (Chrome logs them as "INFO:CONSOLE:<line>] ...").
grep 'CONSOLE' "$RESULTS/chrome.log" | sed -e 's/^.*CONSOLE:[0-9]*\] //' -e 's/, source: .*$//' > "$RESULTS/console.log"
rm -f "$RESULTS/chrome.log"
echo "elapsed $(( $(date +%s) - START ))s; console: $RESULTS/console.log"
tail -20 "$RESULTS/console.log"
exit $STATUS
