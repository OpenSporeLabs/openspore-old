#!/usr/bin/env bash
# The BRP smoke test for OpenSpore, end to end, in one script.
#
# Kept as a script rather than a chain of tool calls because the app has to be
# started, interrogated and shut down while it is definitely alive -- doing it
# by hand across separate invocations is how you end up screenshotting a port
# that closed twenty seconds ago.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

REPO="$(cd "$HERE/../.." && pwd)"
BIN="$REPO/target/debug/openspore"
LOG=/tmp/openspore-brp.log
SHOT=/tmp/openspore-brp-smoke.png
PORT=15702
APP_PID=""

cleanup() {
  # Only ever touches the PID this script started.
  if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    echo "!! cleanup: killing openspore pid $APP_PID"
    kill "$APP_PID" 2>/dev/null
    sleep 2
    kill -9 "$APP_PID" 2>/dev/null
  fi
}
trap cleanup EXIT

echo "### 0. no stale openspore from a previous run"
pkill -f "^$BIN " 2>/dev/null
sleep 1

echo
echo "### 1. launch openspore --placeholder with the BRP bridge"
setsid "$BIN" --placeholder --width 640 --height 480 > "$LOG" 2>&1 < /dev/null &
APP_PID=$!
echo "    pid $APP_PID"

for i in $(seq 1 60); do
  if ss -ltn 2>/dev/null | grep -q ":$PORT"; then break; fi
  if ! kill -0 "$APP_PID" 2>/dev/null; then echo "    !! app died early"; tail -20 "$LOG"; exit 1; fi
  sleep 1
done

if ss -ltn 2>/dev/null | grep -q ":$PORT"; then
  echo "    OK listening on $PORT"
else
  echo "    !! never listened on $PORT"; tail -20 "$LOG"; exit 1
fi

echo
echo "### 2..7 MCP smoke test"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 "$HERE/mcp_call.py" "$(cat <<'JSON'
[
 {"tool":"brp_status","args":{"app_name":"openspore","port":15702}},
 {"tool":"brp_extras_get_diagnostics","args":{"port":15702}},
 {"tool":"world_list_resources","args":{"port":15702}},
 {"tool":"world_get_resources","args":{"port":15702,"resource":"spore_engine::brp::BrpProbe"}},
 {"tool":"world_mutate_resources","args":{"port":15702,"resource":"spore_engine::brp::BrpProbe","path":"label","value":"mutated-by-smoke-test"}},
 {"tool":"world_get_resources","args":{"port":15702,"resource":"spore_engine::brp::BrpProbe"}},
 {"tool":"brp_extras_screenshot","args":{"port":15702,"path":"/tmp/openspore-brp-smoke.png"}},
 {"tool":"brp_execute","args":{"port":15702,"method":"brp_extras/shutdown","params":{}}}
]
JSON
)"
MCP_RC=$?

echo
echo "### 8. screenshot on disk?"
if [[ -f "$SHOT" ]]; then
  ls -la "$SHOT"
  python3 - "$SHOT" <<'PY'
import struct, sys, zlib
d = open(sys.argv[1], 'rb').read()
print('    PNG magic ok:', d[:8] == b'\x89PNG\r\n\x1a\n')
w, h = struct.unpack('>II', d[16:24])
print('    dimensions:', w, 'x', h, ' bytes:', len(d))
# A minimized or occluded surface yields a uniform frame. Distinct byte values
# distinguish "captured the renderer" from "captured a black rectangle".
pos, idat = 8, b''
while pos < len(d):
    ln = struct.unpack('>I', d[pos:pos+4])[0]
    typ = d[pos+4:pos+8]
    if typ == b'IDAT':
        idat += d[pos+8:pos+8+ln]
    pos += 12 + ln
n = len(set(zlib.decompress(idat)))
print('    distinct byte values:', n,
      '-> NON-uniform (real content)' if n > 8 else '-> UNIFORM (suspect blank frame)')
PY
else
  echo "    !! no screenshot at $SHOT"
fi

echo
echo "### 9. clean shutdown?"
for i in $(seq 1 20); do
  kill -0 "$APP_PID" 2>/dev/null || break
  sleep 1
done
if kill -0 "$APP_PID" 2>/dev/null; then
  echo "    !! STILL RUNNING after brp_extras/shutdown"
else
  wait "$APP_PID" 2>/dev/null
  echo "    OK process $APP_PID exited, status $?"
fi
APP_PID=""

echo
echo "### 10. no openspore process remains"
LEFT=$(pgrep -f "^$BIN " | tr '\n' ' ')
if [[ -z "$LEFT" ]]; then echo "    OK none"; else echo "    !! LEFTOVER: $LEFT"; fi
if ss -ltn 2>/dev/null | grep -q ":$PORT"; then echo "    !! port $PORT still bound"; else echo "    OK port $PORT released"; fi

echo
echo "### shutdown lines from the app's own log"
grep -E "No windows are open|Closing window|exiting" "$LOG" | tail -5

exit $MCP_RC