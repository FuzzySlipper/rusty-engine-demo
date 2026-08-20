# E1M1 gameplay calibration

Loading Bay's sole supported scene is the Doom E1M1 Hangar recreation. The WAD is an offline authoring source; Rust owns the admitted runtime and all gameplay consequences.

## Coordinate and content contract

- The port uses 16 Doom map units per Engine unit; the authored E1M1 transform and landmarks are stored in `content/projects/doom-e1m1.project.json`.
- The project carries the map's voxel environment, extracted textures/sprites, and explicit game-owned doors, pickups, enemies, hazards, triggers, and exit. Visible meshes never become collision, navigation, trigger, or combat authority.
- E1M1-specific identities and calibration remain downstream. Reusable Engine mechanisms must not import Doom names or coordinate assumptions.

## Product scope

The playable route covers the Hangar's authored progression, combat, doors, pickups, hazards, secrets, and exit as modeled by this project. It is a bounded recreation, not a Doom engine or a claim to ship Doom runtime code, WAD bytes, sound, music, story text, or trade dress.

## Verification posture

`pnpm run smoke:e1m1` is the focused browser relevance check. `pnpm run certify:e1m1` is the manual/release traversal route; it currently stalls at waypoint `[127,121]`. Treat that as an active limitation until a new observed run supersedes it. Deterministic content admission does not prove the full player route.

Exact source bytes, derived asset manifests, and licensing boundary are maintained in [source-provenance.md](source-provenance.md).
