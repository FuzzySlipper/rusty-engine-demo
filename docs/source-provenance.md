# E1M1 source provenance

Loading Bay ships one authored content closure: `content/projects/doom-e1m1.project.json` and its direct `content/doom-e1m1/` inputs. The WAD is an offline authoring source only. No WAD bytes, Doom runtime code, sound, music, story text, or trade dress are read or shipped at runtime.

## Offline E1M1 source

The forge reads the id Software shareware IWAD at `/home/research/doom.ts/public/doom1.wad`: 4,196,020 bytes, SHA-256 `1d7d43be501e67d927e415e0b8f3e29c3bf33075e859721816f652a526cac771`, `IWAD`, 1,264 lumps. E1M1 is lump 6. The checked intermediate record retains 467 vertices, 475 linedefs, and 138 Things. The WAD's license/status is not changed by this repository; do not infer distribution permission from this record.

`ts/packages/doom-e1m1-authoring` is the project-local deterministic decoder/forge. `doom.ts` is an offline reading reference only; no source file is copied from it. The mapping uses 16 Doom map units per Engine unit. The authoritative product runtime admits the result through Engine services; neither the forge nor the browser evaluates gameplay.

## Derived Doom assets

- `content/doom-e1m1/doom-e1m1.voxel.json` is the authored voxel volume (SHA-256 `59041f8a5e291b844ccc6c17ea404d5842bf2832633014b93f223a9eeea8497b`).
- `content/doom-e1m1/doom-e1m1.asset-catalog.json` is the committed Engine asset-catalog closure for the voxel scene (SHA-256 `3a5e5347b12e522538950ca085e544f00be63b50bff424860cde01b088f25643`). `scripts/generate-e1m1-asset-catalog.mjs` derives its 49 used material/texture pairs offline from the canonical project, texture manifest, and voxel sparse runs; it normalizes stored catalog hashes and preserves exact source paths. Loading Bay admits and resolves this catalog through Engine `AuthoredContent`; it never parses the source-shaped project JSON at runtime.
- `content/doom-e1m1/textures/manifest.json` closes the 54 derived wall/flat PNGs, their source WAD hash, palette hash `fd895921b5d0a394612bb29852ed003d44d69f76dec31c0dc6b5d5fc7d63f7bb`, and each output hash. `SKY1` is an authored equirectangular presentation asset; its source closure and output hashes are in that manifest.
- `content/doom-e1m1/sprites/manifest.json` closes three generated atlases, 198 source lumps, frame identities, and the WAD/palette hashes. Sprite presentation is selected from authoritative product gameplay state; atlas frames do not own combat, collision, or timing authority.
- `content/projects/doom-e1m1.project.json` is the sole canonical project (current SHA-256 `08d069726cdeaf1fddf1181eb3e75d63bad11e5262a4a5b46b4cf9a3bf5ae31b`). `pnpm run check:content` verifies canonical admission and byte-stable regeneration.

## Semantic catalog provenance

The canonical project carries the closed E1M1 item, weapon, player-setup,
pickup, enemy, encounter, hazard, explosive-prop, door, floor, lift, secret,
switch, and level-exit catalogs. Its WAD-derived entities retain placement and
calibrated gameplay values; the product owns the runtime validation, state
transitions, scheduling, facts, and readouts for those entries.

`scripts/generate-e1m1-semantic-catalog.mjs` checks the canonical project hash,
reference closure, and authored/semantic cardinality, then emits the typed
`csharp/LoadingBay.Game/E1M1SemanticCatalog.g.cs` source used by the product.
This is an offline deterministic projection of committed content, not a live
source parser or a second gameplay evaluator. Generic collision, character
motion, spatial queries, presentation, and rendering remain Engine mechanisms;
product policy and E1M1 state remain in the C# product domains.

## E1M1 prop closure

The only non-Doom static-prop sources are colocated under `content/doom-e1m1/props/`. `assets.json` retains their admitted catalogs and material dependencies (current SHA-256 `b49354d8b1442311457c8c2aa89b4647d00c7721a5bac7cb82a1d78992a15704`). `source-manifest.json` is the exact source record (current SHA-256 `6144dc360f65de8c5c1edccfc994862a54a311852d270c62b8f140988e7800b4`): it records each raw source path, hash, source dependency, bounds, material slots, and visual-only collision intent.

Three meshes derive from the retained Kenney Factory Kit 3.0 GLBs under `content/doom-e1m1/props/sources/kenney-factory-kit/`: `security-door`, `hazard-marker`, and `level-exit`. The copied Factory Kit CC0 notice is `content/doom-e1m1/props/KENNEY-FACTORY-KIT-LICENSE.txt` (SHA-256 `61e86565dd297e143ad631594980eda0a17fc81a4cd7c6d71acf2f5e0cad30b6`).

`button-floor-square.glb` is retained from the same Kenney Factory Kit source pack under that license and contains the authored `toggle-on`, `toggle-off`, and `toggle` node clips. The GLB remains byte-for-byte unchanged at SHA-256 `f32def1dd9a57939b096d64361fc5058a8ba240a0394951e8681fb7326ebdeb6`. Its exact Factory Kit 3.0 external image dependency is retained at `content/doom-e1m1/props/sources/kenney-factory-kit/Textures/colormap.png` (512×512 RGBA PNG, SHA-256 `35d7bd6900dde0208429eeaec87fa17fbf024ed59f3f4eab54bc92802eba9dd7`) under the same copied CC0 notice; `source-manifest.json` checks both hashes. At Engine `913a9e665035e6bfdf6ac613cedb62396be4f31d`, landed #7589/#7591/#7595 support the GLB's texture transform, bounded external-image closure, and degenerate visual faces. Loading Bay therefore admits this source directly through `Animation.OpenAnimatedMesh`, retains the named E1M1 `doom-exit` appearance/instance, samples `toggle-off`, and plays `toggle-on` once on the authoritative completion transition. It does not use clip-pack association, a rig, or a local evaluator. The E1M1 closure currently contains no audio clip assets and its gameplay ledger excludes sound/music, so the product retains only a typed Engine SFX bus volume/mute policy and emits no synthetic audio.

The five original low-poly meshes — `energy-cell`, `scatter-shells`, `med-patch`, `impact-vest`, and `breach-scattergun` — retain their own E1M1-local mesh JSON as canonical raw source. Their manifest records the retired historical generator identity only for traceability; no legacy prop kit or generator is required to reimport them.

All eight meshes are visual-only. Collision, navigation, triggers, pickups, doors, hazards, and exit meaning remain explicit admitted game entities. Adding or changing a shipped asset requires updating its direct source closure and this document, then running the content check.
