#!/usr/bin/env bash
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEMO_BIND_HOST=""
DEMO_BIND_PORT=""
DEMO_PROJECT="${RUSTY_ENGINE_DEMO_PROJECT:-content/projects/loading-bay.project.json}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --)
      shift
      ;;
    --host)
      DEMO_BIND_HOST="${2:-}"
      shift 2
      ;;
    --port)
      DEMO_BIND_PORT="${2:-}"
      shift 2
      ;;
    --project)
      DEMO_PROJECT="${2:-}"
      shift 2
      ;;
    *)
      echo "unknown serve-den argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$DEMO_BIND_HOST" ]]; then
  echo "--host is required" >&2
  exit 2
fi
if [[ ! "$DEMO_BIND_PORT" =~ ^[0-9]+$ ]] || (( DEMO_BIND_PORT < 1 || DEMO_BIND_PORT > 65535 )); then
  echo "--port must be an integer from 1 through 65535" >&2
  exit 2
fi
if [[ -z "$DEMO_PROJECT" ]]; then
  echo "--project must not be empty" >&2
  exit 2
fi

cd "$DEMO_ROOT"
DEMO_NX="$DEMO_ROOT/node_modules/.bin/nx"
if [[ ! -x "$DEMO_NX" ]]; then
  echo "workspace dependencies are missing; run pnpm install --frozen-lockfile" >&2
  exit 1
fi
"$DEMO_NX" build loading-bay
exec cargo run --locked -p loading-bay-game --bin browser-host -- \
  --addr "${DEMO_BIND_HOST}:${DEMO_BIND_PORT}" \
  --project "$DEMO_PROJECT"
