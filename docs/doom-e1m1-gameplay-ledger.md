# Doom E1M1 gameplay ledger

Doom E1M1 Hangar is Loading Bay's only supported authored content. Its exact source, derived assets, licensing boundary, and hashes are maintained in [source-provenance.md](source-provenance.md). The port is a bounded reference scene, not a Doom runtime or a claim to ship WAD bytes, music, story text, or trade dress.

## Typed calibration

The active C# policy is intentionally inspectable in `csharp/LoadingBay.Game/LoadingBayTuning.cs`: starting tracks, inventory capacity, movement/look, gravity/jump, camera, spatial scale, E1M1 landmarks, perception, and effect settings are named values rather than host/UI magic numbers. Item/weapon identity is likewise typed in `LoadingBayDefinitions`.

The runtime validates and admits the committed project, voxel, and asset-catalog closure through Engine content services. Engine performs collision, character motion, camera, perception, voxel realization, and rendering. The C# HUD stream exposes bounded state/fact/tuning telemetry without moving gameplay evaluation to the browser.

## Evidence boundary

Focused evidence currently supports Engine-hosted E1M1 rendering, the
one-canvas browser shell, structured HUD projection, and realtime movement
continuation. The observed frame retains a black horizontal band, and repeated
fire while pointer-locked may be ignored after the initial shot. It does not
establish every authored combat, encounter, pickup, door, secret, exit, or full
traversal behavior. Keep future claims tied to direct, current observations.

`pnpm run certify:e1m1` is manual/release work and currently stalls at `[127,121]`; it is an active limitation rather than a passing route.
