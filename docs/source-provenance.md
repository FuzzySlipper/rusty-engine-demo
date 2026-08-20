# E1M1 source provenance

Loading Bay ships one authored content closure: `content/projects/doom-e1m1.project.json` and its direct `content/doom-e1m1/` inputs. The WAD is an offline authoring source only. No WAD bytes, Doom runtime code, sound, music, story text, or trade dress are read or shipped at runtime.

## Offline E1M1 source

The forge reads the id Software shareware IWAD at `/home/research/doom.ts/public/doom1.wad`: 4,196,020 bytes, SHA-256 `1d7d43be501e67d927e415e0b8f3e29c3bf33075e859721816f652a526cac771`, `IWAD`, 1,264 lumps. E1M1 is lump 6. The checked intermediate record retains 467 vertices, 475 linedefs, and 138 Things. The WAD's license/status is not changed by this repository; do not infer distribution permission from this record.

`ts/packages/doom-e1m1-authoring` is the project-local deterministic decoder/forge. `doom.ts` is an offline reading reference only; no source file is copied from it. The mapping uses 16 Doom map units per Engine unit. Rust, not the forge or browser, admits and evaluates the resulting project.

## Derived Doom assets

- `content/doom-e1m1/doom-e1m1.voxel.json` is the authored voxel volume (SHA-256 `59041f8a5e291b844ccc6c17ea404d5842bf2832633014b93f223a9eeea8497b`).
- `content/doom-e1m1/textures/manifest.json` closes the 54 derived wall/flat PNGs, their source WAD hash, palette hash `fd895921b5d0a394612bb29852ed003d44d69f76dec31c0dc6b5d5fc7d63f7bb`, and each output hash. `SKY1` is an authored equirectangular presentation asset; its source closure and output hashes are in that manifest.
- `content/doom-e1m1/sprites/manifest.json` closes three generated atlases, 198 source lumps, frame identities, and the WAD/palette hashes. Sprite presentation is selected from authoritative Rust state; atlas frames do not own combat, collision, or timing authority.
- `content/projects/doom-e1m1.project.json` is the sole canonical project (current SHA-256 `8c3efbe6570d5dfeaf6562f1a672e73849fa4be31ea79bc525e35b45c4186e8e`). `pnpm run check:content` verifies canonical admission and byte-stable regeneration.

## Environmental program provenance

`gameplay/authoring/src/catalogs/environment-programs.ts` is the immutable authored source for the `hazard/nukage` and `explosive-prop/barrel` programs. The committed gameplay package records that file as the provenance source for both catalogs; the project binds every WAD-derived damaging sector and barrel to one of those IDs. The original WAD supplies placement and calibrated damage, radius, and cooldown fields; Rust evaluates overlap, eligibility, cooldown timing, radial targeting, occlusion, scaled damage, causes, chaining, and mutation.

## Encounter program provenance

`gameplay/authoring/src/catalogs/encounter-programs.ts` is the immutable
authored source for `encounter/e1m1`. The package records it as the source for
that catalog, and the forge binds each WAD-derived E1M1 encounter volume to
that ID. The WAD supplies encounter placement and member placement only; Rust
evaluates spatial activation, bound-member lifecycle and cadence, optional
exit-door relations, scheduling, mutation, and event/journal facts.

## E1M1 prop closure

The only non-Doom static-prop sources are colocated under `content/doom-e1m1/props/`. `assets.json` retains their admitted catalogs and material dependencies (current SHA-256 `b49354d8b1442311457c8c2aa89b4647d00c7721a5bac7cb82a1d78992a15704`). `source-manifest.json` is the exact source record (current SHA-256 `4d864378cc495069704ace2f35e3f766b4a2f7e1e1064edce5971c7a0484087e`): it records each raw source path, hash, bounds, material slots, and visual-only collision intent.

Three meshes derive from the retained Kenney Factory Kit 3.0 GLBs under `content/doom-e1m1/props/sources/kenney-factory-kit/`: `security-door`, `hazard-marker`, and `level-exit`. The copied Factory Kit CC0 notice is `content/doom-e1m1/props/KENNEY-FACTORY-KIT-LICENSE.txt` (SHA-256 `61e86565dd297e143ad631594980eda0a17fc81a4cd7c6d71acf2f5e0cad30b6`).

The five original low-poly meshes — `energy-cell`, `scatter-shells`, `med-patch`, `impact-vest`, and `breach-scattergun` — retain their own E1M1-local mesh JSON as canonical raw source. Their manifest records the retired historical generator identity only for traceability; no legacy prop kit or generator is required to reimport them.

All eight meshes are visual-only. Collision, navigation, triggers, pickups, doors, hazards, and exit meaning remain explicit admitted game entities. Adding or changing a shipped asset requires updating its direct source closure and this document, then running the content check.
