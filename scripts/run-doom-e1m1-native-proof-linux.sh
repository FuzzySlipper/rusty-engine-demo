#!/usr/bin/env bash
set -euo pipefail

proof_output=${1:?proof output path is required}
screenshot_output=${2:?screenshot output path is required}
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

export GDK_BACKEND=x11
export LIBGL_ALWAYS_SOFTWARE=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
unset WAYLAND_DISPLAY WAYLAND_SOCKET

./target/debug/native-host --proof-doom-e1m1 >"$proof_output" 2>&1 &
application_pid=$!

cleanup() {
  if kill -0 "$application_pid" 2>/dev/null; then
    kill "$application_pid" 2>/dev/null || true
    wait "$application_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT

for _ in $(seq 1 600); do
  if grep -Fq 'DOOM_E1M1_NATIVE_READY_FOR_CAPTURE' "$proof_output"; then
    break
  fi
  if ! kill -0 "$application_pid" 2>/dev/null; then
    wait "$application_pid"
  fi
  sleep 0.05
done
grep -Fq 'DOOM_E1M1_NATIVE_READY_FOR_CAPTURE' "$proof_output"
sleep 1

import -window root "$screenshot_output"
test -s "$screenshot_output"

wait "$application_pid"
echo "DOOM_E1M1_NATIVE_CAPTURE_OK screenshot=$screenshot_output bytes=$(stat -c %s "$screenshot_output")" >>"$proof_output"
trap - EXIT
