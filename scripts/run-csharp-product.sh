#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
game_project="$repo_root/csharp/LoadingBay.Game/LoadingBay.Game.csproj"
runtime_pack=${RUSTY_RUNTIME_PACK:-"$repo_root/.runtime/runtime-pack-cabba0f"}
rusty=${RUSTY_BIN:-"$runtime_pack/bin/rusty"}
live_debug_args=()
runtime_args=(--runtime "$runtime_pack")

for argument in "$@"; do
  if [[ "$argument" == "--engine-source" || "$argument" == --engine-source=* ]]; then
    runtime_args=()
    break
  fi
done

if [[ "${LOADING_BAY_LIVE_DEBUG:-0}" == "1" ]]; then
  live_debug_args=(--live-debug)
fi

if [[ ! -f "$repo_root/dist/apps/loading-bay/browser/main.js" ]]; then
  printf 'Loading Bay browser bundle is missing. Build the Angular product UI first with: pnpm run build:shell\n' >&2
  exit 1
fi

if [[ ! -x "$rusty" ]]; then
  printf 'Loading Bay requires a Rusty Engine launcher at %s. Set RUSTY_BIN to an explicit rusty executable.\n' "$rusty" >&2
  exit 1
fi

if [[ ${#runtime_args[@]} -ne 0 && ! -f "$runtime_pack/runtime-manifest.json" ]]; then
  printf 'Loading Bay requires a complete matched Rusty Engine runtime pack at %s. Set RUSTY_RUNTIME_PACK to an explicit runtime-pack directory.\n' "$runtime_pack" >&2
  exit 1
fi

exec "$rusty" dev \
  --project "$game_project" \
  "${runtime_args[@]}" \
  --bind-host "${LOADING_BAY_BIND_HOST:-127.0.0.1}" \
  --port "${LOADING_BAY_PORT:-4394}" \
  "${live_debug_args[@]}" \
  "$@"
