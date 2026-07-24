# Source provenance

Rusty Engine Demo was extracted from
[`FuzzySlipper/rusty-engine`](https://github.com/FuzzySlipper/rusty-engine) at commit
`a2e55f9660e46751d4c78bcdd23b9a321b0dc961` under Den task #6137.

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

M10B will append the TypeScript/browser transfer and the exact Kenney license/fixture mapping.
