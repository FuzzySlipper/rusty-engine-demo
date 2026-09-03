#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
game_project="$repo_root/csharp/LoadingBay.Game/LoadingBay.Game.csproj"

node "$repo_root/scripts/generate-e1m1-semantic-catalog.mjs" --check
if [[ ! -f "$repo_root/dist/apps/loading-bay/browser/main.js" ]]; then
  pnpm --dir "$repo_root" run build:shell
fi
dotnet build "$game_project" --nologo
dotnet run --project "$repo_root/csharp/LoadingBay.Game.LifecycleExercise/LoadingBay.Game.LifecycleExercise.csproj" --nologo
dotnet msbuild "$game_project" -t:VerifyRustyEngineAot -p:Configuration=Release --nologo
