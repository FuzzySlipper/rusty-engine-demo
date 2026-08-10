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

window_reference='Doom E1M1 — Rust-native Engine renderer'
# Capture the rendered product window itself. A root-window capture is not
# evidence of the product when Xvfb has no window manager: the mapped 960x640
# native window may not occupy the 640x480 root viewport at all.
capture_candidate=$(mktemp -t doom-e1m1-native-capture.XXXXXX.png)
capture_ready=false
for _ in $(seq 1 40); do
  if import -window "$window_reference" -crop 640x480+0+0 +repage \
    "$capture_candidate" 2>/dev/null && \
    test "$(stat -c %s "$capture_candidate")" -gt 1000 && \
    identify "$capture_candidate" | grep -Fq 'PNG 640x480'; then
    mv "$capture_candidate" "$screenshot_output"
    capture_ready=true
    break
  fi
  sleep 0.25
done
if [[ "$capture_ready" != true ]]; then
  echo "Doom native window never produced a non-trivial 640x480 capture" >&2
  identify "$capture_candidate" >&2 || true
  stat -c 'capture bytes=%s' "$capture_candidate" >&2 || true
  rm -f "$capture_candidate"
  exit 1
fi

wait "$application_pid"
echo "DOOM_E1M1_NATIVE_CAPTURE_OK screenshot=$screenshot_output bytes=$(stat -c %s "$screenshot_output")" >>"$proof_output"
trap - EXIT
