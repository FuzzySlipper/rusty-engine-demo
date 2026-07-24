#!/usr/bin/env bash
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

pnpm run audit:boundary
if rg -n 'GameplayRuntimeHost|GameplayFabric|NativeRuntimeBridge|RuntimeSession|ReactionFrame|DecisionReceipt|ReplayRecord|ProposalEnvelope' rust ts/packages/browser-shell/src ts/packages/project-content/src; then
  echo "forbidden old runtime spine surfaced in active source" >&2
  exit 1
fi
pnpm run typecheck
pnpm run check:content
pnpm run test:ts
pnpm run test:shell
pnpm run build:shell
./scripts/verify-rust.sh
pnpm run test:browser
