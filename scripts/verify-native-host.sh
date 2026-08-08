#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

proof_output=$(mktemp -t loading-bay-native-proof.XXXXXX.log)
rejection_output=$(mktemp -t loading-bay-resource-rejection.XXXXXX.log)
cleanup() {
  status=$?
  if ((status != 0)); then
    echo 'native proof log:' >&2
    tail -n 120 "$proof_output" >&2 || true
    echo 'resource rejection log:' >&2
    tail -n 120 "$rejection_output" >&2 || true
  fi
  rm -f "$proof_output" "$rejection_output"
  trap - EXIT
  exit "$status"
}
trap cleanup EXIT

if [[ "$(uname -s)" != "Linux" ]]; then
  echo 'verify-native-host requires Linux/X11 input automation' >&2
  exit 1
fi

cargo build -p loading-bay-game --bin native-host --locked
xvfb-run -a ./scripts/run-native-host-proof-linux.sh "$proof_output"
xvfb-run -a env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET \
  GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 \
  ./target/debug/native-host --proof-corrupt-resource >"$rejection_output" 2>&1

grep -F \
  'LOADING_BAY_NATIVE_PROOF_OK frame=true views=true camera=true resize=true resource_rendered=true input_authority=true input_noop=true pick_authority=true pick_miss=true state=true render=true save_round_trip=true lifecycle=disposed' \
  "$proof_output"
grep -F \
  'LOADING_BAY_RESOURCE_REJECTION_OK lifecycle=transactional' \
  "$rejection_output"
