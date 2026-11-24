#!/usr/bin/env bash
# Copyright (c) 2026 Christopher Yang
# SPDX-License-Identifier: MIT
#
# Regenerate the screenshots in docs/guide/ from the built frontend.
#
#   tools/screenshots/capture.sh
#
# Needs: Google Chrome (macOS path below, or set CHROME), Node 22+, trunk,
# python3.
# The frontend is built, copied to a temp dir, and served with
# capture-stub.js standing in for the Tauri bridge, so no vault, model,
# or network is involved. Shots are 900x700, the real window size.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/docs/guide"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
PORT="${PORT:-8766}"
WORK="$(mktemp -d)"
trap 'kill "${SERVER_PID:-0}" 2>/dev/null || true; rm -rf "$WORK"' EXIT

(cd "$ROOT/desktop/stern-app" && trunk build >/dev/null)
cp -R "$ROOT/desktop/stern-app/dist/." "$WORK/"
cp "$ROOT/tools/screenshots/capture-stub.js" "$WORK/"
python3 - "$WORK" <<'PY'
import sys, pathlib
w = pathlib.Path(sys.argv[1])
h = (w / "index.html").read_text()
(w / "m.html").write_text(h.replace("<head>", '<head><script src="capture-stub.js"></script>', 1))
PY

if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
  echo "port $PORT is already in use; set PORT=<free port>" >&2
  exit 1
fi
# --directory (not a subshell) so $! is the server itself and the trap can stop it.
python3 -m http.server "$PORT" --directory "$WORK" >/dev/null 2>&1 &
SERVER_PID=$!
sleep 1

# shot "<Image name>" "<query string>"
shot() {
  local name="$1" query="$2"
  node "$ROOT/tools/screenshots/shoot.mjs" "$CHROME" \
    "http://127.0.0.1:$PORT/m.html?$query" \
    "$OUT/User-Guide - $name.png" 900 700
  echo "  $name"
}

mkdir -p "$OUT"
shot "Create Password"  "phase=setup&model=none&fill=Blue-Harbor-2026!"
shot "Choose AI Model"  "phase=unlocked&model=none"
shot "Downloading"      "phase=unlocked&model=downloading"
shot "Unlock"           "phase=login"
shot "Chat Home"        "phase=unlocked"
shot "Save A Note"      "phase=unlocked&play=store%20a%20note"
shot "Typing A Note"    "phase=unlocked&play=store%20a%20note%7CShopping%20list%7Ceggs%2C%20milk%20and%20bread~"
shot "Confirm"          "phase=unlocked&play=store%20a%20note%7CShopping%20list%7Ceggs%2C%20milk%20and%20bread"
shot "Find A Note"      "phase=unlocked&play=store%20a%20note%7CShopping%20list%7Ceggs%2C%20milk%20and%20bread%7C%40yes%7Cfind%20my%20shopping%20list"
shot "Settings"         "phase=unlocked&page=settings"
shot "Help"             "phase=unlocked&page=help"
shot "Dark Theme"       "phase=unlocked&theme=dark"
