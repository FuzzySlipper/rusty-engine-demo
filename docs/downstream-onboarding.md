# Loading Bay onboarding

Loading Bay is a complete-but-narrow ordinary C# consumer example. Copy its ownership boundaries, not its Doom-specific values or content.

## Prerequisites

- Node and the pinned pnpm version for the Angular product UI.
- A .NET SDK for the packaged CoreCLR product and the explicit `linux-x64` NativeAOT check.
- One matched Rusty Engine SDK feed and runtime pack. The local development pair is provisioned under ignored `.runtime/`; select another complete pair explicitly through `RUSTY_RUNTIME_PACK` when needed.

No WAD is required to build or run the committed demo. It is only an offline source for deliberate asset regeneration; preserve the provenance record when that happens.

## Start

```bash
pnpm install --frozen-lockfile
pnpm run build:shell
./scripts/verify-csharp-spine.sh
./scripts/run-csharp-product.sh
```

The final command asks `rusty dev` to build and stage `LoadingBay.Game` through the package, then starts the CoreCLR product with the selected runtime pack. It prints the local browser URL. The package and runtime validate their generated ABI identity before product construction.

Engine contributor work may append `--engine-source /absolute/rusty-engine` to the launcher. That is an explicit source override, not an ordinary downstream setup.

## Copyable shape

| Concern | Loading Bay owner | Rule to copy |
| --- | --- | --- |
| Product policy and state | `csharp/LoadingBay.Game` | Use named, typed C# definitions/tuning/session records. |
| Product composition | packaged `Rusty.Engine` | Declare one entry type plus explicit UI/content/lifecycle facts; generated composition stays below `obj`. |
| Generic runtime mechanisms | Rusty Engine | Use public lifecycle, content, spatial, presentation, UI, and persistence services rather than recreating them. |
| Browser shell | matched runtime pack | Owns browser transport, renderer preload, canvas, input, and lifecycle. |
| Browser UI | `apps/loading-bay` | Export a DOM UI mount and present immutable UI projection only. |
| Authored assets and provenance | `content/` and `docs/source-provenance.md` | Keep source/derived artifact closure distinct from live product state. |

For another product, create its own C# vocabulary and values. Do not import Doom labels, coordinates, content hashes, or Loading Bay policy as an Engine feature. When a missing mechanism is generally Engine-owned, demonstrate the narrow need and promote the seam upstream rather than adding a downstream shim.

## Verification posture

Run `./scripts/verify-csharp-spine.sh` for C# changes. It stages CoreCLR and runs the NativeAOT fidelity target, but neither proves visible browser behavior. For browser changes, build the shell and observe the affected path. Content changes need focused deterministic/provenance checks.

The manual `pnpm run certify:e1m1` route currently stalls at `[127,121]`; it is not an onboarding gate or evidence of a complete route.
