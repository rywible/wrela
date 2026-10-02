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

curl -s -o /dev/null "http://127.0.0.1:$PORT/" || { echo "the server isn't running on $PORT (python3 tools/serve.py $PORT)" >&2; exit 2; }
PROFILE="$(mktemp -d /tmp/wrela-chrome.XXXXXX)"

# One GPU user at a time. On 2026-10-01, ~12 concurrent headless runs starved WindowServer of GPU
# time for over 5 s; macOS's watchdog killed it and the whole desktop reset. Runs now queue on a
# lock (a directory, so creating it is atomic).
#
# A lock whose holder has died is reclaimed by renaming it aside, which only one waiter can do,
# then removing it only if it still names the dead holder; one that changed hands is put back.
# (Removing it in place would race: two waiters that both saw the dead holder could each remove
# it, the slower one removing the lock the faster one had just taken.) A holder is alive if
# `ps -p` finds it, whoever owns it: `kill -0` fails for another user's process.
LOCK=/tmp/wrela-gpu.lock
alive() { ps -p "$1" >/dev/null 2>&1; }
WAITED=0
while ! mkdir "$LOCK" 2>/dev/null; do
  HOLDER="$(cat "$LOCK/pid" 2>/dev/null)"
  if [ -n "$HOLDER" ] && ! alive "$HOLDER"; then
    ASIDE="$LOCK.stale.$$"
    rm -rf "$ASIDE"
    if mv "$LOCK" "$ASIDE" 2>/dev/null; then
      if [ "$(cat "$ASIDE/pid" 2>/dev/null)" = "$HOLDER" ]; then
        rm -rf "$ASIDE"
      elif [ ! -e "$LOCK" ]; then
        mv "$ASIDE" "$LOCK"
      fi
    fi
    continue
  fi
  [ "$WAITED" -eq 0 ] && echo "waiting for the GPU lock (held by ${HOLDER:-?}: $(cat "$LOCK/page" 2>/dev/null))" >&2
  WAITED=$((WAITED + 2))
  [ "$WAITED" -ge 3600 ] && { echo "gave up waiting for the GPU lock after an hour" >&2; exit 3; }
  sleep 2
done
echo $$ > "$LOCK/pid"
echo "$PAGE $HASH" > "$LOCK/page"
# Release the lock only while it's still ours.
trap '[ "$(cat "$LOCK/pid" 2>/dev/null)" = "$$" ] && rm -rf "$LOCK"' EXIT INT TERM
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
