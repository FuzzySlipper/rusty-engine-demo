# Doom E1M1 Voxel Port — High-Level Plan
**Project:** `rusty-engine-demo` @ `5019ade33994bba02e8f0f7112fdfd8cd7e0c730`  
**Date:** 2026-08-07  
**Status:** draft → awaiting approval  
**Source directive:** Port Doom E1M1 concepts into a rusty-engine framework using voxels/voxel primitives + textures, authored in JSON/TS with RS authority, as the proving ground for textured-voxel and spatial capabilities instead of continuing a fake game. Do **not** load `doom1.wad` at runtime.

---

## 1. Goal

Replace the throwaway Loading Bay level-churn loop with one durable, data-driven showcase: a playable reconstruction of **Doom E1M1 “Hangar”** built from **rusty-engine voxel primitives**, surfaced through the existing `rusty-engine-demo` ownership model (RS owns state, TS composes immutable content).

This is not a WAD player. WAD bytes are an **offline source** that a TS toolchain decodes into a canonical `doom-e1m1.project.json` + catalog textures/materials + a single `VoxelAsset` (plus a handful of voxel-object brushes for doors/props). RS admits, simulates, persists, and projects that artifact exactly like it does `loading-bay.project.json` today. The demo proves the textured-voxel owner chain, material-voxel collision/nav/meshing, and the TS→RS authoring boundary that `../rusty-engine` actually needs.

## 2. Success Criteria

- [ ] `content/projects/doom-e1m1.project.json` is a **canonical Studio-owned project** (schema 24) admitted by `ProjectStore`/`project_admission.rs` and round-tripped byte-identically by `check-canonical-projects`.
- [ ] Geometry is **voxel-derived, not mesh literals**: one `VoxelAsset` (sparse runs, `MAX_REPRESENTED_VOXELS ≤ 1M`) holds the E1M1 collision/nav/mesh truth; optional `VoxelObjectAsset`s are reusable brushes (doors, switches) within `MAX_PROJECT_VOXEL_OBJECT_*` budgets. No per-placement baked meshes.
- [ ] **Textures are on voxels**, not on separate face meshes: floor/ceiling flats and wall patches are extracted to RGBA8 PNGs, admitted as `asset-catalog` textures → materials, and bound to voxel material slots with Engine VTX-repeat/atlas-repeat (tile `origin/scale` in cell space, sRGB straight-alpha, filter/mask policy). Browser renderer shows tiled flats on horizontal faces and wall textures on vertical faces through the same greedy-quad path that VTX measured at +32 B/quad.
- [ ] **No WAD at runtime**: `pnpm run build:shell && cargo run -p loading-bay-game --bin browser-host -- --project content/projects/doom-e1m1.project.json` loads the project offline; no `doom1.wad` read, no TS gameplay authority, no generic bridge.
- [ ] Gameplay mapping is object-centric and explicit: player start + angle → `PlayerControllerConfig`; THINGS → existing `Health`/`EnemyCombat`/`Navigation`/`Inventory`/`Pickup`/`Door`/`SecretRegion`/`LevelExit` entities; sectors → height variation; linedefs → walls/doors/switches with explicit proxy bounds. `LoadingBayGameLoop` fixed phases unchanged.
- [ ] Content/TS vs RS split preserved: `ts/packages/project-content` (or new `doom-e1m1-authoring` lib) composes the project; `rust/crates/loading-bay-game/src/{stored_project,content,voxel_object_projection}.rs` admits it and owns mutation. Zero TS state machines.
- [ ] `pnpm run verify` + `pnpm run check:content` green; new focused tests cover WAD decode geometry invariants and texture closure rejection; real Chromium smoke loads the doom project, moves the player, fires weapons, and shows zero `primitive/*` fallbacks.
- [ ] `docs/source-provenance.md` updated with exact WAD SHA-256, byte count, license, every derived PNG/texture/material hash, and the fact that only concepts (geometry topology + texture incidence) are ported.

## 3. Context & Current Facts

**Demo inventory (verified 2026-08-07):**
- `engine-source.json` pinned to `5019ade…` (public Git, one-way). `Cargo.toml` workspace members `loading-bay-game`, `src-tauri`. `[workspace.dependencies]` pulls `asset-catalog`, `core-voxel`, `voxel-asset/convert/object-runtime`, `render-model/projection`, `engine-spatial`, `gameplay-mechanics`, etc. from that rev. `scripts/verify-rust.sh` + `pnpm run verify` are the gates (`docs/extension-recipes.md:10-31`).
- `content/projects/loading-bay.project.json` : schema 24, 89 assets (9 voxel-objects, 0 voxel volumes in the asset chart — the world proxy is a `voxel_volume` in the scene payload), 417 entities; `relay-annex` similar. `check:content` = 8 checks including `check-canonical-projects.mjs` (byte-stable admit+save round-trip).
- Visual-content pipeline (`docs/visual-content-pipeline.md`, `docs/fps-product-architecture.md:43-64`): world proxy is hidden material voxels (`voxelSize 1`, `chunkSize 16`, 3,931 voxels, 8 chunks) + 9 repeated brush objects (25 instances) for visible walls. Proxy supplies collision/nav/occlusion; brushes are appearance only. That split is already the pattern to reuse.
- Engine voxels (`rusty-engine` @ `5019ade…`): `voxel-asset` (sparse runs, `VoxelAssetMaterialBinding`, `VoxelRepresentation::SparseRuns`), `voxel-object-runtime` (admit + `VoxelObjectRenderProjector`), `core-voxel::VoxelPrimitive`, `engine-spatial::VoxelCollisionScene`, `svc-mesh` greedy quads, `render-model/projection` → `renderer-three`. VTX0-6 closeout proves **whole-texture repeat and atlas repeat on voxel faces** with bounded PNG (≤16 MiB, decoded ≤256 MiB, ≤256 texture identities), tile `scale 1/256..4096`, Euclidean remainder, half-texel atlas inset. Demo is *not* yet using that seam — this plan is its first real consumer.

**E1M1 source facts (decoded from `/home/research/doom.ts/public/doom1.wad`):**
- IWAD, 1264 lumps, `doom1.wad` @ `public/doom1.wad`. `doom1wad/` holds the 1197 extracted `.lmp/.raw` but the canonical source is the WAD itself.
- E1M1 at lump 6: 138 THINGS (10 B each), 475 LINEDEFS (14 B), 648 SIDEDEFS (30 B), 467 VERTEXES (4 B), 732 SEGS, 237 SSECTORS, 236 NODES, **85 SECTORS** (26 B), 904 B REJECT, 6922 B BLOCKMAP. Vertex bounds X −768..3808, Y −4864..−2048 (≈4576×2816 doom units). Sector floor/ceiling: min −136, max 264 (first sectors e.g. (0,72), (32,88)…).
- 125 wall textures in `PNAMES`/`TEXTURE1` (e.g. `AASTINKY`, `BIGDOOR2`, `BRNBIGL`…); 54 flats between `F_START`/`F_END` (`FLOOR0_1`, `F_SKY1`, `NUKAGE3`…), each 64×64 raw.
- **E1M1 incidence**: 32 distinct wall textures used, 22 distinct flats (`CEIL3_5`, `FLAT14`, `TLITE6_*`, `F_SKY1`…) — small enough to fit the 256-texture budget without generalizing the seam.

**Doom.ts reference (`/home/research/doom.ts`, note actual path `doom.ts` not `doom1.ts`):** complete TS decoder for `Wad`, `LumpReader`, `VertexArray`/`SectorArray`/`SideArray`/`ThingArray`, `Flat` (64×64 `Uint8ClampedArray`), `TextureArray` (composite via `MapTextureArray` + `PNameArray`), `LEVEL` lump order. `public/doom1wad/E1M1.dat` exists but is the single lump; the WAD remains the hashable source.

**Den:** `mcp__den__list_tasks` returned 0 active tasks — this is a new campaign. Den is reachable (guidance fetched). The plan creates its own task graph.

## 4. Constraints & Non-Goals

**Must honor (`AGENTS.md`, `docs/extension-recipes.md`, `docs/fps-product-architecture.md`):**
- Rust owns live state/validation/mutation/scheduling/facts/persistence/snapshots; TS owns immutable composition/browser input/typed projection/tooling/disposable presentation.
- Object-centric: config/state on entities; named services + explicit `FIXED_TICK_PHASE_ORDER` (Input→PlayerMotion→EnemyIntent→Hazards→Combat→Interactions→Scheduled→Projection).
- One-way pinned Engine dependency; no local sibling path as sole build.
- Game semantics here. Promote an Engine seam only after a *second* real consumer proves reuse. No plugin registry, service locator, behavior IR, generic method bridge, replay/certification spine, or TS gameplay authority.
- Authored project ≠ runtime snapshot ≠ transient presentation. Prove provenance in `docs/source-provenance.md`. Follow owner/test/upstream gates in `extension-recipes.md`.

**Non-goals (this campaign):**
- Not a WAD player, not runtime `WAD`/`PHYSFS` loading, not BSP ray-caster fidelity. Doom is a *reference composition*, not a loading contract.
- Not BSP-accurate lighting/REJECT/BLOCKMAP replication — use Engine's `svc-collision`/`svc-pathfinding` derived from voxels.
- Not full 9×9 Doom episode; first ship is **E1M1 only**. Further maps are data variants if this one lands.
- Not sprite-actor fidelity (Doom sprites remain 8-direction lumps); ship with existing Loading Bay actor kit or low-poly voxel-object proxies. No new sprite-to-voxel pipeline unless the game loop needs it.
- Not mipmapped atlas filtering, compressed GPU formats, or per-face material IDs — the closed VTX contract is sufficient.
- Not a replacement for `loading-bay` — keep that project intact as the regression baseline.

## 5. Key Decisions

| Decision | Options considered | Recommended & why | Rejects |
|---|---|---|---|
| **A. Voxel representation for E1M1 world** | 1) Single `VoxelAsset` sparse volume (hidden proxy + visible tiled faces) 2) Decompose every sector wall into `VoxelObjectAsset` instances 3) Pure `static-mesh` import | **(1) Single `VoxelAsset` for world + tiny `VoxelObject` brush kit for repeats.** The world volume already carries collision/nav/mesh from one source (proven invariant). Objects budget is tight (`65k` resolved cells aggregate) and would force arbitrary partitioning; mesh import loses the proxy guarantee. This also proves both mechanisms: volume for map, objects for doors/switches. | Mesh-only; object-only for world |
| **B. Height & scale mapping** | 16/32/64 doom-units per voxel; anisotropic Y-up vs uniform | **32 doom-units = 1 voxel (≈0.5 m if player 56 → 1.75 m).** Map 4576×2816 → ~143×88 voxels; heights 400 → ~12 voxels. At 64 the steps alias Doom stairs (16-unit steps); at 16 the voxel count explodes (~50k columns × 12 = 600k but with dense columns >400k, near 1M cap and mesher cost). 32 preserves 16-unit stairs as 0.5-voxel? Actually need 16 → so use 16 for vertical quantization but scale XY at 32: anisotropic voxel via translation scaling in projection. Simpler: uniform 16 for fidelity, keep sparse runs — still within 1M, but mesher quads stay 6. Pick **16 uniform** and cap material via greedy merge; document scale. | Anisotropic cell (adds Engine handle) |
| **C. Texture seam** | 1) Decode flats/patches to PNG → admit as `asset-catalog` textures → `StyledMaterial` → bind to `VoxelAssetMaterialBinding` with `tileScale/tileOrigin` repeat 2) Bake patches into per-face meshes 3) Sample source textures offline into vertex colors | **(1) Tiled repeat + atlas where needed.** Exactly the VTX contract: PNG RGBA8 non-interlaced, sRGB straight-alpha, `tileScale 1/256..4096`, atlas content rect with 1-texel replicated gutter for linear, nearest permits 0 padding. One material per distinct flat/wall-texture; voxel slots map incidence. No custom Engine change. | Vertex colors (loses request) |
| **D. Content authority boundary** | TS writes project JSON + referenced PNG bytes; RS validates (`stored_project.rs:validate_*`, `voxel_asset::validate`) + admission | **TS offline authoring script (`ts/packages/doom-e1m1-authoring` or `scripts/doom-e1m1/*`) produces `content/doom-e1m1/*` staging (PNGs + intermediate JSON) → `doom-e1m1.project.json`.** RS `ProjectStore` + `AdmittedVoxelObject` + `VoxelCollisionScene` are the gate; stale hash/aggregate limit/material closure rejects before publication. No runtime WAD read. | TS gameplay decisions |
| **E. Where E1M1 lives** | New scene inside `loading-bay.project.json` vs new canonical project | **New canonical project `content/projects/doom-e1m1.project.json`** (alongside `loading-bay`/`relay-annex`). Keeps baseline stable, lets `check:content` assert stability per-project, and matches `ProjectStore`'s per-project hash. Entry scene `scene/doom-e1m1`. | Mutating existing scene |
| **F. Entity/thing mapping** | Full Doom mobjInfos fidelity vs Loading Bay archetypes | **Map Doom thing `doomedNum` → existing Loading Bay archetypes:** player start (type 1→ `PlayerControllerConfig`), `Doom Imp`→ `Bay Rusher` health/speed, `Shotgun Guy`→ `Arc Warden`, health/armor/ammo things → `pickup` + `InventoryConfig`, keys → `DoorAccessConfig`, barrel → `HazardConfig`. Add one no-new-behavior enemy variant if needed via existing `enemyCombat` config, not new Rust kinds. | New component type per Doom thing |
| **G. Engine gap promotion** | Add E1M1-specific Engine helpers immediately | **None yet.** Use `voxel-asset` sparse runs + `core-voxel::VoxelPrimitive::Extrude` pattern in TS synthesis. Open an Engine task only if a bounded invariant (e.g. “sector-height → voxel extrusion quota”) is proven needed by a **second** consumer. | Premature Engine generalization |

## 6. Recommended Approach (phased narrative)

Think **offline historical forge, not runtime loader**:

```
doom1.wad (hashed source)
   │ TS WAD decoder (reuse doom.ts parse logic, not its runtime) ─┐
   │ extracts: vertexes/sectors/sidedefs/things + flats (64×64 indexed via PLAYPAL) + wall textures (composite patch posts → RGBA)
   ▼                                                               │
staging: /tmp/doom-e1m1-staging  (PNG textures, debug JSON)        │
   │  scale: doom-unit → voxel (16 units = 1 cell), origin at     │
   │  (−768, floor-min, −4864) → voxel [0,0,0]; height extrude per │
   │  sector: for each sidedef, emit vertical sparse runs from     │
   │  floor to ceil; for each sector, emit floor/ceiling runs.    │
   │  Assign material slots by incidence: flat-name→slot, wall-texture-name→slot
   │  Palette → material_asset_id (one PNG per distinct texture/flat instance)
   ▼
voxel asset: DoomE1M1 Volume (SparseRuns, 143×~12×88, ≤250k voxels, ≤32 slots)
   + 3-4 voxel-object brushes (door 2×3×1, switch 1×1×1) with 1-texel atlas if linear
   + catalog materials/textures (32+22 PNGs, ≤~8 MiB; each ≤16 MiB)
   ▼
TS composer → content/projects/doom-e1m1.project.json
   │ entities: player + 30-50 mapped pickups/enemies + 10 doors/keys + exit + lights
   │ scene: one entry scene with voxel instances (world at identity, brushes placed)
   ▼  exact byte check
RS admission: StoredProject schema 24 → VoxelAsset → VoxelCollisionScene → render-model → renderer-three
```

Why this works inside the project contract:
- The **world voxel volume** is the canonical spatial truth (collision/nav/mesh from one source) — the same split the visual-content pipeline already relies on (hidden proxy vs. visible brushes). Here the same volume is both proxy and visible through tiled voxel materials, collapsing the earlier need for duplicated geometry.
- **Textured voxels** are exercised for real: flats tile on horizontal faces, walls repeat on vertical faces via `tileOrigin/tileScale`. The `+32 B/quad` cost is measured on `48×32` and `16×16` corpora — E1M1 at 85 sectors is ~200-400 greedy quads, still tiny.
- **TS never decides gameplay.** It places entities, chooses positions, and maps Doom thing types to *existing* RS configs (`health.max`, `enemyCombat`, `door`, `inventory`, `pickup`). RS validates bounds, duplicate IDs, budgets, material closure, and rejects stale hashes before publishing.

## 7. Work Plan

Den campaign (8 tasks, dependencies listed). All Den tasks use same `project_id: rusty-engine-demo`. Engine pin stays `5019ade…` until the campaign proves a need.

### Task 0 — Campaign scaffold & provenance baseline
*Create parent task and lock the contract.*
- Define parent Den task “Doom E1M1 voxel showcase” (campaign), set `parent_id` for children below.
- Copy verbatim WAD SHA-256 (`sha256sum public/doom1.wad` from `/home/research/doom.ts` plus byte count 4.1 MiB) into `docs/source-provenance.md` skeleton; declare that only geometry incidence + texture incidence are being ported.
- Decide numeric constants: `VOXEL_SCALE = 16 doom units/cell`, `VOXEL_OFFSET = [−768, −136, −4864]`, `MAX_PROJECT_VOXEL_*` already in `stored_project.rs:7-13`.

### Task 1 — Offline WAD→intermediate decoder (TS)
*No RS changes. Fails before any asset is produced.*
- New lib `ts/packages/doom-e1m1-authoring/` (or `scripts/doom-e1m1-decode.mjs`) — reimplements or imports `doom.ts/src/doom/{wad,level,textures}` parse in Node (no browser APIs). Input: `doom1.wad` bytes + `PLAYPAL`/`COLORMAP` lumps. Output: JSON intermediate: vertices (fixed), sectors (floor/ceil/pics/light/special/tag), linedefs/sidedefs (incident texture names), things (x,y,angle,type,options), flats (indexed → RGBA), wall composites (patch → RGBA).
- Deterministic extraction: one JSON per lump, stable sort, `deny_unknown_fields` style. Unit tests assert vertex bound (−768..3808) and counts (85 sectors, 467 vtx…) from §3.
- Tags: `authoring`, `doom-source`.

### Task 2 — Flat & patch → PNG texture pipeline (TS → catalog staging)
*Proves VTX closure before any project exists.*
- Extend decoder with `Flat.decode → 64×64 RGBA PNG` (PLAYPAL lookup) and `TextureArray.generateComposite → W×H RGBA PNG` (patch posts, `R_DrawColumnInCache` logic). Emit one non-interlaced RGBA8 PNG per distinct E1M1 flat/wall texture (32+22 = 54 PNGs; add ~5 extra for DOOR* specials).
- Write staging `content/doom-e1m1/textures/{flat,wall}/*.png` + `manifest.json` (name→sha256→byteCount→contentHash). Run catalog admission dry-run via `asset-catalog`/`voxel-asset` bound from Engine `asset_catalog::StoredMaterialDefinition` + `core-assets::AssetKind::Texture/Material` to verify each PNG ≤16 MiB and total decoded ≤256 MiB, textures ≤256.
- Provenance: each PNG records `source: "doom1.wad:lump:FLATxx"` and `source_sha256`.

### Task 3 — Voxel synthesis: sector extrusion → sparse runs (TS)
*Owns the scale/origin decision. Pure TS before RS.*
- Converter: `doom-e1m1.voxelize(manifest, scale=16, offset)` → `VoxelAsset` sparse runs. Per sector: floor run `z=floor/16 .. ceil/16` for wall columns; sidedef incidence defines vertical quads; allocate `material_slot` per texture: slot 0 = “null”, slots 1..N = flat/wall incidence. Produce `material_palette: [{slot, material_asset_id}]` where each `material_asset_id` will be the catalog material for that texture.
- Budget check: `sparse_runs.length ≤ 1M`, `content_hash` via `with_computed_content_hash`, `bounds: [0,−136/16,0]..[284,16,176]` (example). Emit `doom-e1m1.voxel.json` staging + quota report (voxels, runs, aggregate cells). No RS edit.

### Task 4 — Material & catalog admission scaffolding (RS-light)
*Minimal RS: declare catalog materials, no new component type.*
- In `rust/crates/loading-bay-game` add a `doom_e1m1_materials.rs` helper that registers the 54 `StoredAsset { material, catalog { texture } }` entries with exact `textureAssetId → materialAssetId → voxelSlot` binding and the tiled/atlas mapping (`tileScale = 1/64` for 64×64 flats, `1/width` for wall widths). Uses existing `StoredMaterialDefinition` + `VoxelAssetMaterialBinding` + Engine `asset-catalog` validation.
- Extend `docs/extension-recipes.md` “add a serialized visual asset” path for texture assets (reuses VC5 recipe). Prove lifecycle: `cargo test -p loading-bay-game voxel_…`, plus `check:content` closure rejection (missing texture → diagnostics).
- Gate: if the VTX seam cannot express a needed tiling (e.g. wall y-offset), open an upstream Engine task instead of shimming — keep this task blocked.

### Task 5 — Project composition: `doom-e1m1.project.json` (TS → RS admission)
*The playable artifact.*
- TS composer consumes the intermediate JSON + voxel asset + texture manifest and writes `content/projects/doom-e1m1.project.json` (schema 24): one scene `scene/doom-e1m1` with one voxel volume instance (identity) + 10-15 brush instances (doors/switches), ~60-80 gameplay entities (player at first thing angle, enemies mapped via `MapThing.type→enemyCombat`, pickups via `ItemDefinitionId`, doors via `DoorConfig`+`DoorAccessConfig`, exit via `LevelExitConfig`, secret via `SecretRegionConfig`).
- All entities carry explicit world transforms derived from Doom coords (`x>>FRACBITS` scaled by 1/16, y→z, floor height → y). Collision bounds stay on entities; voxel identity remains gameplay truth.
- Call `scripts/check-canonical-projects.mjs` style check: admit via `ProjectStore`, canonicalize, round-trip, require byte equality. `pnpm run check:content` must include the new project.

### Task 6 — Browser presentation & scene selection
*No gameplay authority moves to TS.*
- Extend `apps/loading-bay` routing or `libs/project-content` to list available projects (`loading-bay`, `relay-annex`, `doom-e1m1`) and allow `--project` / landing-page card to launch the doom scene. `RuntimeProjectionAdapter` already maps voxel objects → retained frame; add doom-specific palette-to-material mapping test in `ts/packages/browser-shell/src/view-model.test.ts` style.
- Disposable polish (bob, muzzle, HUD) reuse existing descriptors; no new renderer import. Prove `mountRendererSurface` still has one scheduler and one canvas per scene.

### Task 7 — Verification, performance & certification
*Make it reviewer-cold-startable.*
- Focused Rust: `enemy_archetype_runtime`, `project_codec`, `stored_project`, `voxel_edit_persistence` covering the new project.
- Content: `pnpm run check:content` (8→9 checks), `node scripts/check-brush-level.mjs` style alignment check for doom voxels.
- Headed: `pnpm run test:browser` loads `doom-e1m1`, traverses sector 0→exit, fires weapons, asserts ≥22 flat materials and ≥32 wall materials rendered, `RendererSurface.submission()` stats.
- Docs: `docs/evidence/doom-e1m1-authoring.json` + screenshots (`cadetblue` baseline vs `doom-e1m1` draw-call/triangle counts), update `docs/source-provenance.md` and Den `known-limitations`.

**Dependency DAG:** 0 → 1 → 2 → 3 → (4, 5) → 6 → 7. Tasks 4 and 5 parallel after 3 but 5 blocks on 4's material IDs.

## 8. Validation Plan

| Surface | Command / check | Evidence |
|---|---|---|
| TS decode | `pnpm --filter @rusty-engine-demo/doom-e1m1-authoring test` | 85 sectors, 32 walls/22 flats, vertex bounds, PLAYPAL determinism |
| Textures | `node scripts/doom-e1m1-textures.mjs --check` / dry-run `asset-catalog` | 54 PNGs non-interlaced RGBA8, each ≤16 MiB, total decoded ≤256 MiB, ≤256 identities; stale hash → rejection |
| Voxel asset | `cargo test -p loading-bay-game --lib vox` + `with_computed_content_hash` | sparse runs ≤1M, `content_hash`/`voxel_data_hash` stable, bounds within `[i64]` |
| Project admiss | `pnpm run check:content` + `check-canonical-projects` | `doom-e1m1.project.json` admit+save = byte-identical; aggregate limits: objects ≤256, instances ≤4096, frames ≤8193, resolved cells ≤65536 |
| Rust suite | `./scripts/verify-rust.sh` / `cargo test --locked -p loading-bay-game` + clippy | zero new unsafe, no regressions in `navigation_runtime`, `pickup_runtime`, `progression_runtime` |
| Cross-lang | `pnpm run verify` (boundaries, typecheck, tsc content, engine pin) | `engine-source.json` stays `5019ade…`; no sibling path |
| Headed | `pnpm run test:browser` + `scripts/browser-smoke.mjs` with `--project doom-e1m1` | loads, player moves (sectors floor variation), enemies take damage, door opens on switch interaction, exit fact emitted |
| Studio | `cargo run -p loading-bay-game --bin studio-adapter` + real Studio `@ 127.0.0.1:4396` | doom project opens at 1 shared canvas, hierarchy lists voxel volume + brushes, select & move brush → reread hash stable |
| Perf | `scripts/certify-headed-performance.mjs` | draw calls/triangles documented in `docs/performance.md`; note VTX +32 B/quad overhead |
| Provenance | `docs/source-provenance.md` diff | WAD SHA-256 + 54 texture SHAs + voxel `content_hash` listed with license/byte counts |

**Highest-risk step:** Texture extraction fidelity (PLAYPAL indexing + patch column posts). A single off-by-one in `R_DrawColumnInCache` or palette lookup will make flat validation pass locally but the renderer will show wrong colors/alpha — gate this with a golden PNG byte-hash regression (check 2-3 known flats against `doom.ts` rendering output) before any voxel is bound.

## 9. Risks / Rollback

| Risk | Impact | Mitigation / rollback |
|---|---|---|
| Scale choice pushes voxel count >1M or mesher cost skyrockets. | Build rejects with `project.voxelObjectAggregateLimit`. | Keep `VOXEL_SCALE` configurable in the TS composer; switch 16→32 via one constant and regenerate. No RS change needed. |
| Wall patch composite mis-handles `widthMask`/patch origin causing seams. | Visual seams, shader clamping artifacts. | Validate composite PNG against `doom.ts` canvas output at 1:1; fall back to single-patch textures for affected walls. |
| PLAYPAL/colormap drift (different Doom palette revision). | Wrong flat hues. | Pin palette lump bytes SHA-256 in manifest; treat mismatch as hard error. |
| Material closure incomplete (some sidedef references texture not extracted). | `check:content` rejects with `project.missingAsset`. | Decoder enumerates sidedef incidence first; fail fast if incidence > extracted. |
| Gameplay feel wrong (Doom 56-unit player vs Loading Bay 0.5-unit bounds). | Navigation hapless, doors too narrow (Doom door 64 wide = 4 voxels at 16). | Map thing types to existing `HealthConfig.hitboxHalfExtents`/`NavigationConfig.speed` via TS tuning constants; keep RS policy unchanged. Document mapping in `doom-e1m1-authoring/readme.md`. |
| Adding a third Engine pin churns locks. | `Cargo.lock`/`pnpm-lock.yaml` drift. | Pin stays `5019ade…` for this campaign; any VTX follow-up is a separate `engine:update` task with evidence. |
| **Rollback:** delete `content/projects/doom-e1m1.project.json` + `content/doom-e1m1/` staging + catalog materials; rerun `pnpm run check:content` — prior two canonical projects remain green. No DB migration, no snapshot migration needed. | — | — |

## 10. Open Questions

1. **Doom angle → entity yaw mapping:** `MapThing.angle` (0 = west per old Doom? actually 0 = east) must be mapped to `playerController.initialYawDegrees` and `EnemyCombatConfig` facing. Verify against `doom.ts/src/doom/play/mobj` or accept “east = 0°” and document.
2. **Vertical quantization of stairs/doors:** Many Doom steps are 8 or 16 units. Uniform 16 hides 8-unit steps; is that acceptable for a proof, or do we keep vertical at 8 while XY at 32? Decision: defer to Task 3 spike with visual evidence of Floor7_* steps.
3. **Sky handling:** E1M1 ceilings with `F_SKY1` are visual sky, not voxel. Render as retained skybox vs omitted voxel. Proposal: omit voxel for `F_SKY1`, let renderer background color show through; revisit only if playtest demands.
4. **Doom.ts license (GPL-3.0) vs extraction tooling:** The WAD is id Software shareware data; `doom.ts` parsing logic is GPL-3.0. We are not shipping `doom.ts` code, only using it as a *reference* for patch/PLAYPAL decode — confirm that newly written decoder is clean-room (attribute idea, not line-copy) or record GPL tooling boundary.
5. **Things count:** 138 things include decorations. For first demo, map only the ~30 gameplay-relevant things (player, monsters, weapons, keys, health/armor). Leave barrels/lights as visual landmarks or omit — scope gate for Task 5.

---

### What to decide now (before tasks are created)

- Approve **uniform 16 units/cell + single `VoxelAsset` + 54 tiled materials** as the shipped shape. Alternative (32 or anisotropic) remains a one-line regenerate fallback.
- Confirm that **a new canonical project** (`doom-e1m1.project.json`) is preferred over mutating `loading-bay`. If you want an in-place scene, Task 5 collapses to “new scene in existing project”.
- Confirm that **we do not need a new Engine pin** for this campaign — current `5019ade` already has VTX6.

*Consultant note:* a bounded `architecture-consult` was spawned (`gpt-5.6-sol` high) but its packet truncated on the wire (summary delivered, artifacts were `ls` probes). The synthesis above was verified manually against `AGENTS.md`, `stored_project.rs`, `voxel_object_projection.rs`, `docs/visual-content-pipeline.md`, `rusty-engine/docs/design.md`, VTX closeout, and live WAD header/lump parsing. Re-running the consultant with a tighter scope (“texture binding only”) is recommended if Task 2-4 reveal closure friction.
