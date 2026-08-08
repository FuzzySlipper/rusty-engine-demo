#!/usr/bin/env bash
set -euo pipefail

proof_output=${1:?proof output path is required}
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

export GDK_BACKEND=x11
export LIBGL_ALWAYS_SOFTWARE=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
unset WAYLAND_DISPLAY WAYLAND_SOCKET

./target/debug/native-host --proof >"$proof_output" 2>&1 &
application_pid=$!

cleanup() {
  if kill -0 "$application_pid" 2>/dev/null; then
    kill "$application_pid" 2>/dev/null || true
    wait "$application_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT

for _ in $(seq 1 600); do
  if grep -Fq 'LOADING_BAY_NATIVE_READY_FOR_INPUT' "$proof_output"; then
    break
  fi
  if ! kill -0 "$application_pid" 2>/dev/null; then
    wait "$application_pid"
  fi
  sleep 0.05
done
grep -Fq 'LOADING_BAY_NATIVE_READY_FOR_INPUT' "$proof_output"
sleep 0.5

if command -v xdotool >/dev/null 2>&1; then
  window_id=$(xdotool search --name 'Loading Bay' | head -n 1)
  xdotool mousemove --window "$window_id" 400 300
  xdotool mousedown 1
  sleep 0.3
  xdotool mouseup 1
  xdotool keydown Escape
  sleep 0.2
  xdotool keyup Escape
  sleep 0.2
  xdotool keydown Return
  sleep 0.2
  xdotool keyup Return
else
  python3 ./scripts/x11-send-loading-bay-proof-input.py
fi

wait "$application_pid"
trap - EXIT
