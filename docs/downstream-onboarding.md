# Loading Bay onboarding

Loading Bay is a complete-but-narrow C# NativeAOT consumer example. Copy its ownership boundaries, not its Doom-specific values or content.

## Prerequisites

- Adjacent Rusty Engine checkout at `../rusty-engine`.
- Node and the pinned pnpm version for the Angular shell.
- .NET SDK capable of publishing the `linux-x64` NativeAOT product.
- Rust toolchain for the Engine `csharp-product-runtime` development host.

No WAD is required to build or run the committed demo. It is only an offline source for deliberate asset regeneration; preserve the provenance record when that happens.

## Start

```bash
pnpm install --frozen-lockfile
pnpm run build:shell
./scripts/verify-csharp-spine.sh
./scripts/run-csharp-product.sh
```

The final command publishes `LoadingBay.NativeProduct` and starts the adjacent Engine C# product runtime using the Angular bundle and committed content root. It prints the local browser URL. Use a separate disposable persistence root when experimentation must not retain saves.

## Copyable shape

| Concern | Loading Bay owner | Rule to copy |
| --- | --- | --- |
| Product policy and state | `csharp/LoadingBay.Game` | Use named, typed C# definitions/tuning/session records. |
| Native boundary | `csharp/LoadingBay.NativeProduct` | Keep the composition root microscopic and use Engine-generated public contracts. |
| Generic runtime mechanisms | Rusty Engine | Use public lifecycle, content, spatial, presentation, UI, and persistence services rather than recreating them. |
| Browser shell | `apps/loading-bay` | Capture semantic input and present immutable UI projection only. |
| Authored assets and provenance | `content/` and `docs/source-provenance.md` | Keep source/derived artifact closure distinct from live product state. |

For another product, create its own C# vocabulary and values. Do not import Doom labels, coordinates, content hashes, or Loading Bay's policy as an Engine feature. When a missing mechanism is generally Engine-owned, demonstrate the narrow need and promote the seam upstream rather than adding a downstream shim.

## Verification posture

Run `./scripts/verify-csharp-spine.sh` for C# changes. For browser changes,
also build the shell and observe the affected path through the Product Browser
Host. Content changes need focused deterministic/provenance checks.

The manual `pnpm run certify:e1m1` route currently stalls at `[127,121]`; it is not an onboarding gate or evidence of a complete route.
