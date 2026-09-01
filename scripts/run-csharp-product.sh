#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
engine_root=${RUSTY_ENGINE_ROOT:-"$repo_root/../rusty-engine"}
bundle_dir=${LOADING_BAY_BUNDLE_DIR:-"$repo_root/dist/apps/loading-bay/browser"}
content_dir=${LOADING_BAY_CONTENT_DIR:-"$repo_root/content"}
native_dir="$repo_root/.engine-development/csharp-native-product"
persistence_root=${LOADING_BAY_PERSISTENCE_ROOT:-"$repo_root/.engine-development/persistence"}
live_debug_args=()

if [[ "${LOADING_BAY_LIVE_DEBUG:-0}" == "1" ]]; then
  live_debug_args=(--live-debug)
fi

if [[ ! -f "$bundle_dir/index.html" ]]; then
  printf 'Loading Bay browser bundle is missing at %s. Build the retained shell first with: pnpm run build:shell\n' "$bundle_dir" >&2
  exit 1
fi

if [[ ! -d "$content_dir" ]]; then
  printf 'Loading Bay content directory is missing at %s.\n' "$content_dir" >&2
  exit 1
fi

if [[ ! -f "$engine_root/rust/crates/csharp-product-runtime/Cargo.toml" ]]; then
  printf 'Rusty Engine csharp-product-runtime is missing under %s. Set RUSTY_ENGINE_ROOT to the adjacent Engine checkout.\n' "$engine_root" >&2
  exit 1
fi

mkdir -p "$native_dir" "$persistence_root"
dotnet publish "$repo_root/csharp/LoadingBay.NativeProduct/LoadingBay.NativeProduct.csproj" \
  -c Release \
  -r linux-x64 \
  --self-contained true \
  -o "$native_dir"

exec cargo run --manifest-path "$engine_root/rust/crates/csharp-product-runtime/Cargo.toml" --locked --bin csharp-product-runtime -- \
  --library "$native_dir/LoadingBay.NativeProduct.so" \
  --bundle-dir "$bundle_dir" \
  --content-dir "$content_dir" \
  --persistence-root "$persistence_root" \
  --bind-host "${LOADING_BAY_BIND_HOST:-127.0.0.1}" \
  --port "${LOADING_BAY_PORT:-4394}" \
  --mode realtime \
  "${live_debug_args[@]}" \
  --direct-intent player.move.forward=digital \
  --direct-intent player.move.left=digital \
  --direct-intent player.move.backward=digital \
  --direct-intent player.move.right=digital \
  --direct-intent player.jump=digital \
  --direct-intent player.use=digital \
  --direct-intent player.fire=digital \
  --direct-intent player.look.x=axis \
  --direct-intent player.look.y=axis \
  --physical-mapping player.move.forward=player.move.forward:key:key-w:held \
  --physical-mapping player.move.left=player.move.left:key:key-a:held \
  --physical-mapping player.move.backward=player.move.backward:key:key-s:held \
  --physical-mapping player.move.right=player.move.right:key:key-d:held \
  --physical-mapping player.jump=player.jump:key:space:pressed \
  --physical-mapping player.use=player.use:key:key-e:pressed \
  --physical-mapping player.fire=player.fire:pointer-button:primary:pressed \
  --physical-mapping player.look.x=player.look.x:pointer-axis:x \
  --physical-mapping player.look.y=player.look.y:pointer-axis:y
