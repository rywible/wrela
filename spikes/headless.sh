#!/bin/sh
# Runs a spike page in headless Chrome stable (real GPU, throwaway profile) and waits for it to
# finish. The page signals completion by PUTting results/DONE (any body). Console output from the
# page goes to <spike>/results/console.log, so WGSL compile errors and exceptions are visible.
#
# Usage: spikes/headless.sh <spike-dir> [hash] [timeout-seconds]
#   spikes/headless.sh 03-forest '#quick' 120
#
# Needs the spikes server on 127.0.0.1:8417 (python3 spikes/serve.py 8417).
# Timing taken while other GPU work runs is indicative only.

set -u
SPIKE="${1:?usage: headless.sh <spike-dir> [hash] [timeout-seconds]}"
HASH="${2:-#run}"
TIMEOUT="${3:-600}"
ROOT="$(cd "$(dirname "$0")" && pwd)"
RESULTS="$ROOT/$SPIKE/results"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
PROFILE="$(mktemp -d /tmp/wrela-chrome.XXXXXX)"

mkdir -p "$RESULTS"
rm -f "$RESULTS/DONE"
curl -s -o /dev/null "http://127.0.0.1:8417/" || { echo "spikes server isn't running on 8417" >&2; exit 2; }

"$CHROME" --headless=new --user-data-dir="$PROFILE" --no-first-run --no-default-browser-check \
  --disable-extensions --disable-background-timer-throttling --disable-renderer-backgrounding \
  --enable-logging=stderr --v=0 --window-size=1920,1080 \
  "http://127.0.0.1:8417/$SPIKE/$HASH" 2> "$RESULTS/chrome.log" &
PID=$!

START=$(date +%s)
STATUS=1
while :; do
  if [ -f "$RESULTS/DONE" ]; then STATUS=0; break; fi
  if ! kill -0 "$PID" 2>/dev/null; then echo "chrome exited early" >&2; break; fi
  if [ $(( $(date +%s) - START )) -ge "$TIMEOUT" ]; then echo "timed out after ${TIMEOUT}s" >&2; break; fi
  sleep 1
done
kill "$PID" 2>/dev/null
sleep 1
kill -9 "$PID" 2>/dev/null
rm -rf "$PROFILE"

# Page console lines only (Chrome logs them as "INFO:CONSOLE:<line>] ...").
grep 'CONSOLE' "$RESULTS/chrome.log" | sed -e 's/^.*CONSOLE:[0-9]*\] //' -e 's/, source: .*$//' > "$RESULTS/console.log"
rm -f "$RESULTS/chrome.log"
echo "elapsed $(( $(date +%s) - START ))s; console: $RESULTS/console.log"
tail -20 "$RESULTS/console.log"
exit $STATUS
