# Source provenance

Rusty Engine Demo was originally extracted from
[`FuzzySlipper/rusty-engine`](https://github.com/FuzzySlipper/rusty-engine) at commit
`a2e55f9660e46751d4c78bcdd23b9a321b0dc961` under Den task #6137.

The current Rust and browser dependencies resolve from exact Engine revision
`8cb49db6cfe9471faa23ab0661656a2366a83d8c`. The older revision below remains the historical
extraction point, not the active dependency pin.

## M10A Rust transfer

| Local surface | Source path | Treatment |
|---|---|---|
| `rust/crates/loading-bay-game` | `rust/crates/game-host` | Copied as one cohesive gameplay vertical; package/crate imports renamed from `game-host`/`game_host`. |
| `content/projects` | `content/projects` | Copied unchanged for loading-bay and converted-content admission/product behavior. |
| `content/generated` | `content/generated` | Copied unchanged for migration, encounter, controller, navigation, and workload tests. |
| `content/assets/kenney-wall-a.voxel.json` | same path | Copied unchanged as canonical converted-asset test input. |

Reusable Rust crates are not copied. Cargo consumes their packages directly from the exact Engine
Git revision recorded in `Cargo.toml` and `Cargo.lock`.

The original Engine repository contains the historical Asha donor provenance for low-level code.
This repository records its immediate Engine source and does not recreate the old donor hierarchy or
runtime claims.

## M10B browser transfer

| Local surface | Source path | Treatment |
|---|---|---|
| `ts/packages/project-content` | same path | Copied as the optional immutable content composer and renamed to `@rusty-engine-demo/project-content`. |
| `ts/packages/browser-shell` | same path | Copied as the product-owned input, projection, feedback, and browser shell; imports renamed to the demo package scope. |
| `ts/packages/render-contracts` | same path | Initially copied into the demo; removed under #6162 after its complete successor became a shared exact-revision Engine package. |
| `ts/packages/renderer-three` | same path | Initially copied into the demo; removed under #6162 after the retained Three/WebGL backend and browser surface moved behind shared Engine packages. |
| `scripts/browser-smoke.mjs` | same path | Copied as the end-to-end product proof; the Rust package invocation changed from the source product name to `loading-bay-game`. |
| Root pnpm, TypeScript, and Vite configuration | same paths | Copied and narrowed to the demo-owned package identities and verification gate. |

The browser packages initially moved together because all four served one product at extraction
time. The later renderer migration made the demo an external consumer of
`@rusty-engine/render-contracts`, `render-projection`, `renderer-host`, and `renderer-three`.
Only the game-specific input, semantic projection, and typed fact-to-descriptor mapping remain here.

The CC0 conversion source is copied byte-for-byte into `fixtures/voxel-conversion` so the demo can
inspect the source named by its persisted converted-wall provenance:

- `kenney-wall-a.glb`: 3,352 bytes, SHA-256
  `6fceda24c30d2c22694f232f03fe2115fb1a462046fbbf719a90eea10dc9af00`
- `KENNEY-RETRO-URBAN-KIT-LICENSE.txt`: 318 bytes, SHA-256
  `3679c62e69e67da74fec17327635e67c92991ac82b0bdfcc203d8ecd473c016a`

The Engine copies remain in their repository because Engine's converter/provider tests consume
them independently. This downstream copy is licensed and source-traceable; it is not a path link.

## M10C native downstream extension

`ExtractionBeacon` and project schema 8 were authored directly in this repository after the
transfer. The component family, named service, direct runtime entry point, typed fact, admission,
snapshot persistence, browser readout, and game-specific presentation mapping have no corresponding
source path in Rusty Engine. Updating to the later shared renderer revision added no Engine gameplay
vocabulary, so this extension remains evidence of downstream ownership rather than another copied
product surface.

`content/projects/relay-annex.project.json` is likewise native downstream content. It is generated
from the TypeScript `relayAnnexStoredProject` composition and admitted by the already-existing Rust
project path and headless beacon proof; it was not transferred from Engine.
