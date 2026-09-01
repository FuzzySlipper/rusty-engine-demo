#!/usr/bin/env bash
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEMO_BIND_HOST=""
DEMO_BIND_PORT=""
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
cd "$DEMO_ROOT"
LOADING_BAY_PORT="$DEMO_BIND_PORT" \
  LOADING_BAY_BIND_HOST="$DEMO_BIND_HOST" \
  LOADING_BAY_LIVE_DEBUG=1 \
  exec "$DEMO_ROOT/scripts/run-csharp-product.sh"
