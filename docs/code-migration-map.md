# Loading Bay product map

Loading Bay is a focused C# NativeAOT reference product hosted by Rusty
Engine. Doom E1M1 is its sole authored experience. The map below names the
current owner for each concern and the surface future downstream agents can
inspect or tune.

## Current ownership map

| Concern | Current owner | Observable/tunable surface |
| --- | --- | --- |
| Product lifecycle and NativeAOT entry | `LoadingBayProduct`, `LoadingBay.NativeProduct` | Engine product lifecycle and generated NativeAOT boundary. |
| E1M1 policy and product state | `LoadingBaySession`, `LoadingBayDefinitions`, `LoadingBayTuning` | Typed tracks, inventory/equipment, facts, snapshots, named tuning, and debug receipts. |
| Save composition | `LoadingBaySnapshotCodec` plus Engine persistence primitives | Versioned C# snapshot identity and validation. |
| Content closure | `LoadingBaySession` and `LoadingBayVoxelScenePresentation` | Exact committed project, voxel, asset-catalog, texture, sprite, and prop references admitted through Engine content services. |
| Movement and spatial view | `LoadingBaySession` plus Engine spatial/camera services | Named E1M1 spawn and movement tuning; Engine owns character steps, collision, camera, and perception. |
| World interactions | `LoadingBayWorldServices` plus Engine spatial/presentation services | Typed hazards, barrels, doors, lifts, secrets, encounters, exit state, schedules, and interaction facts. |
| HUD | `LoadingBayHudProjection` and Angular HUD | `loading-bay.hud.snapshot.v1`, copied read-only presentation data, and bounded structured telemetry. |
| Browser integration | public Product Browser Host | Closed semantic inputs, one Engine canvas, and Engine-owned realtime lifecycle. |

## Content and evidence boundary

The offline E1M1 forge may regenerate deterministic derived assets and
manifests from the recorded source closure. The C# runtime admits committed
artifacts through Engine content services and consumes the generated typed
semantic catalog; it does not parse source-shaped authoring data at runtime.

The HUD makes health/armor, ammunition, generation, admitted-step, facts/drop
telemetry, world schedules, and named tuning visible. Focused product proof
currently covers lifecycle, NativeAOT construction, Engine-hosted rendering,
the one-canvas frame, HUD projection, and realtime continuation. The visible
browser capture retains a black horizontal band, and repeated pointer-locked
fire may be ignored after the initial shot. The manual E1M1 certifier stalls
at waypoint `[127,121]`; it is release/manual work and not a verification gate.

When extending the product, add game-specific policy beside its typed C# owner,
use public Engine mechanisms for generic runtime behavior, and record focused
proof for the affected visible path or content closure.

## Focused gates

`./scripts/verify-csharp-spine.sh` is the default C# proof: managed build,
lifecycle exercise, and NativeAOT publish. `pnpm run build:shell` covers the
Angular presentation. Content/provenance changes use the deterministic checks
described in [source-provenance.md](source-provenance.md).
