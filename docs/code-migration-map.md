# Loading Bay product map

Loading Bay is a focused C# product hosted by a matched Rusty Engine runtime pack. Doom E1M1 is its sole authored experience. The package generates the CoreCLR and NativeAOT binds below `obj`; the tracked source contains only product code.

## Current ownership map

| Concern | Current owner | Observable/tunable surface |
| --- | --- | --- |
| Product entry and staging | `LoadingBayProduct` plus packaged `Rusty.Engine` | One explicit product type, staged UI/content roots, CoreCLR normal lane, NativeAOT fidelity target. |
| E1M1 policy and product state | `LoadingBaySession`, `LoadingBayDefinitions`, `LoadingBayTuning` | Typed tracks, inventory/equipment, facts, snapshots, named tuning, and debug receipts. |
| Save composition | `LoadingBaySnapshotCodec` plus Engine persistence primitives | Versioned C# snapshot identity and validation. |
| Content closure | `LoadingBaySession` and `LoadingBayVoxelScenePresentation` | Exact committed project, voxel, asset-catalog, texture, sprite, and prop references admitted through Engine services. |
| Movement and spatial view | `LoadingBaySession` plus Engine spatial/camera services | Named E1M1 spawn and movement tuning; Engine owns character steps, collision, camera, and perception. |
| World interactions | `LoadingBayWorldServices` plus Engine spatial/presentation services | Typed hazards, barrels, doors, lifts, secrets, encounters, exit state, schedules, and interaction facts. |
| HUD | `LoadingBayHudProjection` and Angular HUD | `loading-bay.hud.snapshot.v1`, copied read-only presentation data, and bounded telemetry. |
| Browser integration | matched Engine runtime pack | Closed semantic inputs, renderer preload, one Engine canvas, and realtime lifecycle. |

## Content and evidence boundary

The offline E1M1 forge may regenerate deterministic derived assets and manifests from the recorded source closure. The C# runtime admits committed artifacts through Engine content services and consumes the generated typed semantic catalog; it does not parse source-shaped authoring data at runtime.

The HUD makes health/armor, ammunition, generation, admitted-step, facts/drop telemetry, world schedules, and named tuning visible. `./scripts/verify-csharp-spine.sh` is the focused C# proof: semantic catalog, Angular staging when necessary, managed build, lifecycle exercise, and the package-generated NativeAOT check. It does not replace a focused browser observation when UI behavior changes.

The visible browser capture retains a black horizontal band, and repeated pointer-locked fire may be ignored after the initial shot. The manual E1M1 certifier stalls at waypoint `[127,121]`; it is release/manual work and not a verification gate.
