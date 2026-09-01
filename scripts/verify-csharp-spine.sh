#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
native_dir="$repo_root/.engine-development/csharp-native-product"

node "$repo_root/scripts/generate-e1m1-semantic-catalog.mjs" --check
dotnet build "$repo_root/csharp/LoadingBay.Game/LoadingBay.Game.csproj" --nologo
dotnet run --project "$repo_root/csharp/LoadingBay.Game.LifecycleExercise/LoadingBay.Game.LifecycleExercise.csproj" --nologo
dotnet publish "$repo_root/csharp/LoadingBay.NativeProduct/LoadingBay.NativeProduct.csproj" \
  -c Release \
  -r linux-x64 \
  --self-contained true \
  -o "$native_dir" \
  --nologo
