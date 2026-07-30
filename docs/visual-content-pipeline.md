# Loading Bay visual-content pipeline

This document freezes the pre-replacement visual baseline for Den campaign
`rusty-engine-demo #6350`. The evidence revision is
`cd25485445bfb581c4005b221a23caa21408d327`, with Rust, renderer, and Studio packages pinned to
Rusty Engine `198dccaa3f6b15d776b58d0f60c0f025e4b12171`.

Task #6351 imports no assets and changes no runtime authority. Its purpose is to make the current
placeholder implementation, candidate sources, ownership decisions, and comparison measurements
explicit before later tasks replace content.

Raw structured measurements are in
[`docs/evidence/visual-content-placeholder-baseline.json`](evidence/visual-content-placeholder-baseline.json).

## Authority map

| Concern                                                             | Canonical owner                                                                                   | Serialized source                                           | Runtime consumer                                                     | Must not become an owner                                   |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- | -------------------------------------------------------------------- | ---------------------------------------------------------- |
| Project and scene composition                                       | Loading Bay stored-project schema admitted and published by Rust                                  | `content/projects/loading-bay.project.json` after VC3       | Rust project/session construction and Studio                         | A second TypeScript scene or browser storage               |
| Asset identity, source hash, dependencies, bounds, clips, materials | Canonical project asset catalog plus copied asset/license files                                   | Project `assets` and `content/assets`                       | Rust admission, Studio inspection, shared renderer projection        | Rust mesh/voxel literals or a browser asset registry       |
| Static and animated source conversion                               | Rust-owned Studio adapter composing exact-pinned Engine providers                                 | Prepared plan plus atomically published project/asset files | Studio and Rust project store                                        | Ad hoc Blender output copied without provenance            |
| Voxel-brush definitions                                             | Canonical Engine voxel-object definitions admitted by Rust                                        | Project voxel-object assets                                 | Studio editing/placement and shared projection                       | One browser-owned world voxel grid                         |
| Voxel-brush instances and transforms                                | Entity-owned project instances admitted by Rust                                                   | Scene entities and voxel-object instance records            | Studio hierarchy, project readout, shared renderer                   | Unique baked mesh copies for every placement               |
| Gameplay meaning and live state                                     | Loading Bay Rust services and fixed-tick phases                                                   | Authored gameplay components plus runtime snapshot/save     | Typed immutable browser projection                                   | Asset names, animation clips, or TypeScript state machines |
| Collision, navigation, occlusion, hitboxes, triggers                | Rust project admission and canonical Engine spatial services                                      | Explicit entity/world proxy components                      | Rust gameplay loop                                                   | Decorative mesh triangles or fine visual voxels            |
| Animation posture                                                   | Rust-owned enemy/weapon/progression state and facts                                               | Typed visual binding in the canonical project               | Shared renderer animation facilities through disposable presentation | A browser animation clock that decides gameplay            |
| Camera-relative weapon presentation                                 | Shared Engine `viewmodel` layer; Loading Bay supplies serialized assets and disposable transforms | Canonical viewmodel asset references                        | One shared `RendererSurface`                                         | A private Three scene, loader, scheduler, or picking path  |
| Rendering, resources, picking, timing                               | Exact-pinned Rusty Engine render packages                                                         | Renderer-neutral retained descriptors                       | One auto-started `RendererSurface`                                   | A demo renderer, resource cache, or second frame loop      |
| Performance evidence                                                | Reproducible scripts plus exact-revision evidence                                                 | `docs/performance.md` and `docs/evidence`                   | CI and headed desktop certification                                  | HUD counters as gameplay authority                         |

### Visual and gameplay proxy boundary

The replacement work deliberately separates appearance from gameplay geometry:

- The material voxel environment is now an explicit hidden `gameplayProxy` supplying canonical
  collision, navigation, and occlusion. The detailed repeated brush kit is visible content only
  and cannot silently become a second gameplay truth.
- Doors retain their Rust-owned `collision`, `kinematic`, `bounds`, `door`, access, and occlusion
  behavior. A closed/open mesh follows the admitted door state and transform.
- Enemies retain Rust-owned kinematic bounds, health hitboxes, navigation, perception, attacks,
  encounter state, and drops. An actor armature or animation never selects a target or deals damage.
- Pickups retain Rust-owned bounds, item identity, quantity, collection, visibility, and
  consumption. Mesh scale is not pickup range.
- Hazard and secret regions remain explicit Rust-owned bounds even if their visible prop or floor
  treatment is larger, smaller, or absent.
- Encounter contacts are invisible Rust-owned trigger entities. Authored lights are real retained
  light descriptors, not mesh placeholders.
- First-person viewmodels remain camera-relative, excluded from world picking/collision, and
  rebuildable after reset, replacement, or disposal.

Every task that changes a visual transform must prove the related proxy still aligns through the
canonical project readout and normal gameplay route.

## Canonical project ownership

VC3 establishes this ownership:

1. `content/projects/loading-bay.project.json` becomes the canonical durable Loading Bay visual and
   scene artifact.
2. `ts/packages/project-content` retains typed schemas, helpers for deliberately generated fixtures,
   migrations, and semantic assertions. It no longer contains a second complete Loading Bay visual
   scene that must equal the Studio project.
3. `check:content` parses, canonicalizes, Rust-admits, round-trips, and checks stable semantic
   invariants of the canonical project. It does not compare the file with
   `loadingBayStoredProject()`.
4. `content/generated` contains only explicit migration/workload fixtures. Both Loading Bay and
   Relay Annex under `content/projects` are canonical project artifacts; generation commands
   cannot overwrite either file.
5. `scripts/check-canonical-projects.mjs` uses the Rust `ProjectStore` and complete admission path
   to save each canonical artifact to a disposable directory, then requires exact byte equality.
   The check leaves the project and caller worktree unchanged.

The former full `loadingBayStoredProject()`/`relayAnnexStoredProject()` composition and its
generator equality test have been removed. Stable gameplay identities remain exported only as
small test/fixture constants; the serialized project is the sole complete scene.

## Serialized prop-kit result

VC5 #6354 replaces every visible non-actor gameplay primitive with 17 canonical static-mesh
assets. Eight are bounded derivatives of the local Kenney CC0 Factory/Industrial source files and
nine are original deterministic mesh derivatives for pickups, weapons, and their muzzle flash.
Studio protocol 11 imported all 17 source derivatives, published 26 gameplay-entity appearance
mappings, added two decorative landmarks, reread the canonical project, and reconstructed the same
bytes through a fresh adapter process.

The VC5 publication hash was
`81dd321e11f7aeb458e4a7aa5760ca2d37adc65f0265a956bd8919bddd54e770`. The prop kit contains
8,127 vertices and 2,709 triangles. Its exact sources, derivative hashes, bounds, material slots,
license hashes, and collision intent are recorded in
`content/assets/prop-kit/source-manifest.json`; the Studio receipt is
`docs/evidence/prop-kit-authoring.json`.

| Serialized family      | Project assets                                                                          | Durable use                                                |
| ---------------------- | --------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Industrial interaction | security door, control panel, hazard marker, extraction beacon, level exit, status unit | 12 stateful route/progression entities                     |
| Supplies and equipment | energy cell, scatter shells, med patch, impact vest, maintenance pass                   | 14 placed or enemy-drop pickup entities                    |
| Weapon silhouettes     | arc pistol, breach scattergun, rivet carbine, muzzle flash                              | World pickups plus one camera-relative retained viewmodel  |
| Visual-only landmarks  | overhead crane, coolant tank                                                            | Two serialized scene landmarks with no gameplay components |

The browser receives admitted material and static-mesh resources from Rust. Only the player marker
and the two enemy archetypes retain the explicitly bounded primitive fallback, pending VC4 #6355.
An absent non-actor mesh now rejects projection instead of silently becoming a colored cube or
sphere. Door, switch, pickup, hazard, beacon, and exit appearance variants are selected from typed
Rust component state; asset-name matching no longer owns gameplay presentation state.

### Environment

The scene retains the coarse material-voxel environment as a hidden `gameplayProxy` and retains the
nine-definition/25-instance VC6 proof room off the playable route. The gameplay proxy has:

- `voxelSize: 1`, `chunkSize: 16`;
- 3,931 authored material voxels across addresses `[0,0,0]` through `[30,4,51]`;
- material-slot counts 2,243 / 1,612 / 76;
- eight canonical collider/navigation chunks.

The proxy is original Loading Bay content, but it is not emitted as the complete visible room.
VC7 #6357 rebuilds that appearance from repeated fine-grid object-local brush instances while
preserving the proxy addresses and all gameplay identities.

### Camera-relative viewmodels

`ts/packages/browser-shell/src/weapon-viewmodel.ts` maps each authoritative equipped weapon to one
canonical serialized mesh and uses the serialized muzzle-flash mesh for attack presentation. The
shared Engine `viewmodel` root makes both children camera-relative and excludes them from world
picking. One equipped weapon retains three handles rather than seven inline primitive parts.
Movement bob, recoil, flash visibility, reset, and disposal remain disposable transforms derived
from accepted Rust state and facts; they cannot alter aim, ammunition, damage, or cooldown.

### Public Studio and game proof

The public Studio shell reconstructed the exact project at Engine revision
`9813bf6f759a8967a5de1681d4726f7b17254ca5` with 89 assets, 74 entities, one shared canvas, and no
renderer error. Selecting the overhead crane and coolant tank through the public hierarchy raised
the submitted frame from 12 draw calls / 340 triangles to 66 draw calls / 312,646 triangles and 62
draw calls / 312,688 triangles respectively. Geometry/material resources remained bounded at
49/63. Resize at 1280×720 and 1600×900, route disposal to zero canvases, remount, reload, and
post-reload selection all passed. The immutable shell-output sample and screenshots are in
`docs/evidence/prop-kit-studio-browser.json` and the adjacent `prop-kit-studio-*.png` files.

The real browser campaign selected all three Rust-owned weapons after their ordinary world
pickups. Its presentation evidence contains `weapon/arc-pistol`, `weapon/breach-scattergun`, and
`weapon/rivet-carbine`; no undefined-asset error or downstream definition cache is accepted. The
shared-surface stress sample moved from 50 draw calls / 53 live handles / 15,357 triangles to
82 / 86 / 15,421, then returned to 50 / 53 / 15,357 after cleanup. Four reusable geometry and
material definitions intentionally remained resident at 47/96 after their live instances were
removed, exercising the exact Engine #6416 lifetime contract.

The schema-11 converted-wall browser fixture first migrates through the Rust project store, then
imports its three serialized prop sources and publishes five fixture appearances through Studio
protocol 11. This keeps the old-schema proof real without restoring primitive non-actor fallbacks
or introducing a second asset path.

### Intentional non-mesh entities

These are not missing visual assets:

- encounter contacts 2, 40, and 50;
- secret region 31;
- ambient light 80 and point lights 81–87.

The contacts and secret region are invisible gameplay proxies. The lights are retained authored
light descriptors. Later tasks may add nearby visible landmarks but must not turn those meshes into
the trigger/light authority.

## Candidate source inventory

The preferred local source root is `/home/stash/mesh-resources` (plural). All shortlisted packs are
created and distributed by Kenney, identify the author as `Kenney` / `www.kenney.nl`, and declare
[CC0 1.0](http://creativecommons.org/publicdomain/zero/1.0/). The supplied metadata links to
`https://kenney.nl/` and Kenney's 3D import documentation. These are candidates only; no file below
is shipped by #6351.

| Pack                      | Version / metadata      | Candidate use                                                                       | License SHA-256                                                    |
| ------------------------- | ----------------------- | ----------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Animated Characters Retro | 1.1                     | Base armature/model, idle/run/jump clips, human/zombie skins                        | `eaa916e20df30c26f18a752290f93ab0e5d95c3dd1057e6887d11aa4acc0e74b` |
| Blocky Characters         | 2.0; created 2025-06-10 | Alternate pre-exported GLB actor silhouettes                                        | `610fec89c16826112e9d6b80497b726c43fea0e42c9cd9d7cb081f8ad550c0ec` |
| Factory Kit               | 3.0; created 2026-05-01 | Doors, screens, switches, hazard signs, scanner/beacon, machinery, crates, catwalks | `61e86565dd297e143ad631594980eda0a17fc81a4cd7c6d71acf2f5e0cad30b6` |
| City Kit Industrial       | 1.0; created 2025-06-25 | Tanks, chimneys, large exterior/landmark silhouettes                                | `bf1195a387c996ab4bb6d05bb7ead8c5b233c0532634fec916ef9e090936c3e5` |

### Actor inputs

| Local candidate                                                 | SHA-256                                                            | Intended treatment                                                     |
| --------------------------------------------------------------- | ------------------------------------------------------------------ | ---------------------------------------------------------------------- |
| `kenney_animated-characters-retro/Model/characterMedium.fbx`    | `18835fef534eede635b081ee7fe647d01a885550a591d2e6bf071010906167d8` | Blender armature/model source                                          |
| `.../Animations/idle.fbx`                                       | `c8a24e0294376ee5a195c56752a13310e1c0b5f8588a4db50e094120e3e4cc74` | Merge as named idle clip                                               |
| `.../Animations/run.fbx`                                        | `e635461fc8dace85ec67a7f7941e949a7c3f108b51ae4d2da1557e6e01749df8` | Merge as named locomotion clip                                         |
| `.../Animations/jump.fbx`                                       | `b88429077a7a1af5d3f55f43cfd8ce0f7441b4f6f7bb15a8070d7ed15d275f74` | Candidate reaction/death source only if the reviewed mapping is honest |
| `.../Skins/humanMaleA.png`                                      | `1590e08cea37f5aecbacabb40a57c176e389e9a95d5b2a4de00086604ef23e1c` | Bay Rusher material candidate                                          |
| `.../Skins/zombieMaleA.png`                                     | `e0dab7c762bfc63e0fa29e4cad769bead9d990fb0c15a5d7aea2ba0b08d82c92` | Arc Warden material candidate                                          |
| `kenney_blocky-characters_20/Models/GLB format/character-a.glb` | `8ee5dae167ec589863f6bba222467eb90ace8be357a4c5abfcab289290181616` | Static/alternate silhouette comparison                                 |
| `.../character-r.glb`                                           | `b880654e0bcf4cfda119750d1ae0842ccfb73ae22f7844ab95015122b746808a` | Second alternate silhouette comparison                                 |

The installed Blender baseline is 5.1.2, build
`ec6e62d40fa9`. VC4 must record the exact Blender version, import/export settings, axes, scale,
pivot, material handling, clip ranges/names, source hashes, and output GLB hashes. If attack, hit, or
death clips are authored or derived, their changes must be reproducible and described rather than
hidden behind renamed stock clips.

### VC4 actor source checkpoint

The reproducible actor source stage is now checked in under `content/assets/actor-kit`. Blender
5.1.2 produces two independently skinned 1.78-unit GLBs from the reviewed medium rig and installs
the exact six-clip set `idle`, `run`, `jump`, `attack`, `hit`, `death`. Kenney owns the first three
clips and both source skins under CC0; Loading Bay owns the explicit recipe-defined attack, hit, and
death root actions. The completed GLBs are 339,812 and 334,232 bytes and retain one skinned mesh,
one material, and one embedded texture each.

This checkpoint intentionally stops at the supported authoring boundary. Protocol-11
`prepareAssetImport` currently returns typed `assetImport.sourceNotUtf8` for the binary GLB because
the public Engine importer admits textual static mesh sources only. Rusty Engine #6433 owns the
bounded binary animated-mesh import, persistence, replacement, and preview seam. VC4 will pin that
reviewed provider and publish through Studio; it will not decode GLB, hand-author retained payloads,
or load Three.js privately downstream.

### Prop and landmark inputs

All paths below are under `kenney_factory-kit_3.0/Models/GLB format` unless noted.

| Candidate                                               |  Bytes | SHA-256                                                            | Proposed use                                      |
| ------------------------------------------------------- | -----: | ------------------------------------------------------------------ | ------------------------------------------------- |
| `door-wide-closed.glb`                                  |  3,572 | `987837d7b45bca466c5f2268fa0193e014374cc0d2ef0784f6c37092856b71d0` | Closed security door                              |
| `door-wide-open.glb`                                    |  3,000 | `bc8cfb0d899c512d85241c41db4247be11cfe833cab56949f19e2903789133d9` | Open-state comparison or Blender-combined door    |
| `screen-panel-wide.glb`                                 | 16,000 | `092066a198f1f81c857ceab23497dd3ea6066d261a91497cf0f83f7ba951c4d0` | Interlock/control panel                           |
| `lever-single.glb`                                      | 17,516 | `d7056a698ecb46c972d51848a77c8a9aca86f085cc5800347fce1c19c6c59477` | Switch silhouette                                 |
| `button-floor-square.glb`                               |      — | `f32def1dd9a57939b096d64361fc5058a8ba240a0394951e8681fb7326ebdeb6` | Hazard/pressure marker                            |
| `warning-orange.glb`                                    | 16,136 | `07974e9e78c2b2d2ed8198461a7fbac86d1744490ab687817543a07f163aa7d0` | Hazard and route warning                          |
| `scanner-high.glb`                                      | 21,656 | `b71d8ab86fe1a12542eac14e4185e17babf15ac086c3343acf8600a080ed985a` | Extraction beacon base                            |
| `indicator-special-arrow.glb`                           |  2,500 | `3cd514de3e283705df2baccf7cea62a70ba64e87311252fe26788be4377c0d49` | Exit/route marker                                 |
| `box-small.glb`                                         |  7,500 | `fd2ea1ac4f24a9515dc9c53a4a589a8897c824fa5865771799f965a252f684f0` | Ammunition/supply container base                  |
| `machine.glb`                                           | 25,620 | `a39e3042bcb7789274428357383317d70e1c31906e5301c99e7d9e90ac584863` | Generator-room landmark                           |
| `crane.glb`                                             | 53,396 | `ceaf20fb976ce0415d2b3d40e723b6ad377845818fa4fa68b55434108b1a6880` | Loading-bay landmark                              |
| `catwalk-straight.glb`                                  | 33,492 | `85797c6ce53e3f4373bc59ecd3ce951a3f6b707651def5e0bae46756051cd18d` | Elevated route landmark, not level-grid authority |
| `City Kit Industrial/Models/GLB format/detail-tank.glb` | 23,456 | `b1edc2953c590c16f1d8280dfeb9073af6c710ac3d587f3dba144f69363d799b` | Generator/extraction silhouette                   |

Health, armor, keys, ammunition variants, weapon pickups, and the three first-person weapons should
prefer original Studio-authored voxel objects or Blender derivatives whose differences are visible
and whose source steps are committed. A generic crate with asset-name color branching is not an
acceptable final replacement.

## Voxel-brush experiment

VC6 compares at least two treatments at equal world dimensions:

| Treatment           | Candidate local voxel size | Purpose                                                                                             | Required evidence                                                                                                          |
| ------------------- | -------------------------: | --------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Conservative relief |            1/16 world unit | Readable trim, panel insets, vents, door surrounds, floor/ceiling bands with restrained cells/faces | Source cells, resolved cells, worst-case faces, meshed vertices/indices/groups, bytes, resource reuse, authoring usability |
| Dense relief        |            1/32 world unit | Intentional texture-like modeled detail and renderer/mesher pressure                                | Same metrics plus aliasing/readability and repeated-instance stress                                                        |

The proof room must reuse definitions for straight wall, compatible corner treatment, doorway,
panel/vent, column, floor, ceiling, and landmark modules. It cannot bake every placement into a
unique asset. The current project limits are hard admission boundaries, not targets:

- 256 voxel-object definitions;
- 8,193 total frames;
- 65,536 total resolved cells;
- 393,216 worst-case mesh faces;
- 4,096 instances.

Accepted content stays below those bounds with explicit headroom. One-over tests remain fail-closed
before materialization.

### VC6 authored result

Task #6356 authors an original nine-definition Loading Bay brush kit through Studio protocol 11:
source import, GLB inspection, private conversion preview, atomic apply, placement prepare, durable
attach, canonical reread, and a fresh adapter process. The canonical project hash after publication
is `dde061a1b27fdb8665bc0d7099a5ec364310272e618f23f6c6177a7bb8a6393a`.

The proof room is outside the playable route at world extent `[31, 0, 3]`…`[35, 3, 7]`. Its 25
instances reuse nine definitions; they include rotation, normalized scale, and one material
override. Instance owners intentionally have no bounds, collision, kinematic, trigger, hazard, or
secret component. The existing coarse material-voxel environment and explicit entity proxies
remain the only gameplay collision, navigation, and occlusion truth.

| Module            | Cell | Intended dimensions | Repeats |  Cells | Worst faces | Vertices / indices | Expanded mesh | Stored object | Prepare / response |
| ----------------- | ---: | ------------------: | ------: | -----: | ----------: | -----------------: | ------------: | ------------: | -----------------: |
| Wall conservative | 1/16 |        2 × 2 × 0.25 |       6 |  2,228 |      13,368 |     7,000 / 10,500 |       210 KiB |       6.4 KiB |   220 ms / 293 KiB |
| Wall dense        | 1/32 |        2 × 2 × 0.25 |       2 | 16,386 |      98,316 |   81,400 / 122,100 |     2,385 KiB |      98.3 KiB |   881 ms / 3.3 MiB |
| Corner            | 1/16 |           2 × 2 × 2 |       4 |  4,729 |      28,374 |    35,664 / 53,496 |     1,045 KiB |      93.5 KiB |   924 ms / 1.4 MiB |
| Doorway           | 1/16 |       3 × 2.5 × .25 |       1 |    944 |       5,664 |      6,200 / 9,300 |       182 KiB |       9.0 KiB |   850 ms / 280 KiB |
| Vent panel        | 1/16 |        2 × 2 × .375 |       1 |  2,201 |      13,206 |    10,704 / 16,056 |       314 KiB |      10.4 KiB |   905 ms / 450 KiB |
| Column            | 1/16 |       .75 × 2 × .75 |       4 |  1,776 |      10,656 |    13,168 / 19,752 |       386 KiB |      29.8 KiB |   974 ms / 552 KiB |
| Floor strip       | 1/16 |        2 × .375 × 2 |       4 |  2,968 |      17,808 |    19,280 / 28,920 |       565 KiB |      20.5 KiB | 1,075 ms / 790 KiB |
| Ceiling strip     | 1/16 |          2 × .5 × 2 |       4 |  3,760 |      22,560 |    22,080 / 33,120 |       647 KiB |      25.4 KiB | 1,157 ms / 900 KiB |
| Relay landmark    | 1/16 |           2 × 2 × 1 |       1 |  7,274 |      43,644 |    53,184 / 79,776 |     1,558 KiB |      84.2 KiB | 1,505 ms / 2.1 MiB |

The full kit resolves 42,266 cells—64.5% of the 65,536 project limit—and has a conservative
253,596-face worst-case estimate, 248,680 actual projected vertices, 373,020 indices, and about
7.1 MiB of expanded mesh payload. The canonical project is 2,450,553 bytes. Attaching each instance
through the intentionally atomic Studio mutation/reread path took 139.6 seconds total and up to
5.89 seconds for the last, 12.3 MiB response. This is editing-path evidence, not renderer frame
time: repeated publication currently re-admits and reprojects the growing project.

The 1/32 wall has the same placed `2 × 2 × 0.25` dimensions as the 1/16 wall, but uses 7.35× the
cells, 7.35× the face work, 11.63× the expanded mesh bytes, 15.44× the stored object bytes, and
11.63× the vertices. Its narrow trim, alternating recessed channels, and raised center relief are
visible at ordinary Studio camera distance, but the extra bands alias sooner while moving and the
editing/response cost is disproportionate.

**Production decision for VC7:** use 1/16-world-unit cells and conservative relief for repeatable
walls, corners, floors, ceilings, and structural modules. Reserve 1/32 only for a small number of
focal inserts where the silhouette or close-range relief is materially better. Keep world-space
module dimensions on a 0.25-unit sub-grid and normalize placement scale when conversion occupancy
does not fill the authored grid. This retains modeled detail while leaving roughly 23,000 resolved
cells of project headroom for VC7 iteration.

The real browser reconstruction uses one shared Studio canvas and reports nine definitions, 25
instances, 119 retained operations, a valid selected render handle, no placement ghost, and no
renderer error. The immutable browser readout and heap sample are in
[`docs/evidence/voxel-brush-kit-studio-browser.json`](evidence/voxel-brush-kit-studio-browser.json);
the complete conversion, admission, response, and reconstruction measurements are in
[`docs/evidence/voxel-brush-kit-authoring.json`](evidence/voxel-brush-kit-authoring.json).
Screenshots:

- [complete proof room](evidence/voxel-brush-kit-studio-overview.png);
- [1/32 dense relief](evidence/voxel-brush-kit-dense-wall.png);
- [1/16 conservative relief](evidence/voxel-brush-kit-conservative-wall.png).

Renderer-owned observations come from the public
`StudioShellComponent.frameSubmitted` event at exact Engine revision
`70808ba1b74b908c47edfbf3b1282fb2eb5f192d`. The event is emitted only after the accepted Studio
frame is explicitly submitted and `RendererSurface.submission()` is read. The downstream app
retains at most 32 immutable events for evidence; it does not access WebGL, Three, the child
viewport, or a second render loop. A focused public-package consumer test covers complete,
incremental, and presentation updates. The real browser route emitted complete and presentation
updates through the shell:

| Shared-surface submission                        | Draws | Triangles | Handles | Geometries | Materials | Textures | Animated | Backend submit |
| ------------------------------------------------ | ----: | --------: | ------: | ---------: | --------: | -------: | -------: | -------------: |
| Initial complete, default camera                 |     9 |        96 |      62 |         32 |        33 |        0 |        0 |        23.0 ms |
| Dense wall selected/focused, presentation        |    28 |   310,852 |      62 |         32 |        33 |        0 |        0 |        12.8 ms |
| Conservative wall selected/focused, presentation |    28 |   310,852 |      62 |         32 |        33 |        0 |        0 |        10.8 ms |
| Project closed then remounted, complete          |     9 |        96 |      62 |         32 |        33 |        0 |        0 |         8.6 ms |
| Fresh page reload then dense focus, presentation |    28 |   310,852 |      62 |         32 |        33 |        0 |        0 |        13.2 ms |

Draws and triangles are per submission. Handles and resources are live-resident counts. Selecting
either repeated wall keeps the same 32 geometry resources: 25 placements reuse the nine admitted
brush definitions rather than uploading one geometry per placement. The content-rich focus raises
the submitted triangles because the proof room enters the camera frustum; it does not change
resident resources.

The lifecycle run preserved exactly one canvas and a ready/no-error renderer at 1280×720 and
1600×900, reached zero canvases after **Close Project**, returned to one after **Open**, and again
reconstructed one after a cache-bypassing page reload. The bounded recorder retained three events
across child disposal, then received a new complete event from the remounted child. After the final
reload/focus, Chromium reported 689,616,760 bytes of JavaScript heap. That value follows several
full 2.45 MiB project reconstructions in one evidence run and is not a native runtime footprint.
The synchronous backend durations are CPU-side submission evidence under headless SwiftShader,
not GPU completion time. Event-to-event intervals are dominated by deliberate camera focus,
reconstruction, and browser automation and are not presented as frame cadence.

## VC7 repeated-brush Loading Bay

The playable Loading Bay is now composed from 342 new durable instances of the same nine VC6
definitions. The off-route 25-instance proof room remains for exact comparison, giving 367
instances and 419 serialized entities in the scene. No per-placement asset copy or
downstream mesh cache exists.

`scripts/author-loading-bay-brush-level.mjs` derived placement transforms from the admitted
gameplay-proxy addresses and the canonical door entities, then published 11 ordered Rust-owned
Studio protocol-12 batches with at most 32 create-only placements apiece. Each accepted batch
performed complete candidate admission, deterministic owner allocation, one atomic project
publication, and one canonical readout. The project changed from
`477cfd0fc44385710e0049398c2f23de17bb72a2a6905d665e1df1cb0f04557e` to
`3a1518c45e7201865c5dc4c04b5e8d2a77be5ad760d3449fef3d56d327feb1ae`; same-process reread and a
fresh adapter both reconstructed that exact hash.

| Shared definition | Playable repeats |
| ----------------- | ---------------: |
| Ceiling strip     |               28 |
| Floor strip       |               28 |
| Conservative wall |              215 |
| Dense wall insert |               38 |
| Vent panel        |               16 |
| Doorway surround  |                5 |
| Column            |                6 |
| Corner            |                4 |
| Relay landmark    |                2 |

Every new owner is decorative: it has no collision, kinematic, trigger, door, switch, hazard,
secret, pickup, or enemy component. Each of the five canonical doors has exactly one named
doorway-surround instance, while door collision/occlusion and open/closed behavior remain on the
original Rust-owned entity. Floors, ceilings, wall relief, columns, corners, and landmarks likewise
cannot affect motion or rays.

The exact Engine line combines atomic batch placement, canonical greedy same-material coplanar
meshing, and copy-on-write retained-projection staging at
`5a42db2feac72788b25eedf8d5efbc0fb2ec2afd`. Greedy meshing reduced the complete structural frame
to 739,471 bytes, below the 2 MiB product bound, while source-face quota charging remains
unchanged. The real Studio route reconstructed nine definitions and 367 instances through one
shared `RendererSurface`. Its initial complete submission reported 123 draws, 34,514 triangles,
412 handles, 49 geometries, and 63 materials. Selecting two distinct conservative-wall owners
kept the same geometry/material counts, proving individual picking over shared resources.

Exact Engine descendant `e0e97de882c7fdb8b6b35e4c282713a31fc133b2` adds renderer-owned
moving-camera visibility compaction without changing the retained model or this authored scene.
The scene-wide `c903c1c86761386087acd7d7d814a3da5cde116b` intermediate disabled culling; the
later `e97944c8309018f595222edb7bd90a620c32cedf` revision restored it but placed all 367
project instances into only three 32-unit cells. The 8-unit
`6fe4713df76ce0a03a6c461dfa95d4a90b24c824` revision split the same scene into 129
cell-and-definition groups, matching its broad 131-draw diagnostic submission. Exact CI rejected
all three intermediates.

The current provider instead retains bounded definition-compatible candidates and filters their
members for the current camera immediately before BrowserSurface and Studio submission. Its
representative nine-definition/367-instance provider regression yields nine draw groups while all
367 logical identities remain retained and pickable. In the unchanged local
headless-SwiftShader normal-control campaign, cadence was 16.7 ms, maximum authoritative command
RTT was 1,192.5 ms, queue peaks were 1/1/1, and no facts were dropped. The rebuilt explicit
submission measured 40 draws for 412 retained handles, down from the rejected 131-draw
fixed-cell result. Exact CI acceptance remains
recorded by the task gate rather than inferred from this workstation run. This replaces neither the
nine definitions nor any of the 342 playable placements.

Close reached zero canvases; open, resize at 1280×720 and 1600×900, cache-bypassing reload, and
selection after reload each returned one ready/no-error canvas. Exact evidence is in
[`voxel-level-brush-authoring.json`](evidence/voxel-level-brush-authoring.json) and
[`voxel-level-brush-studio-browser.json`](evidence/voxel-level-brush-studio-browser.json).
Screenshots:

- [complete playable brush scene](evidence/voxel-level-brush-studio-overview.png);
- [selected conservative wall owner](evidence/voxel-level-brush-wall-primary.png);
- [second repeated conservative wall owner](evidence/voxel-level-brush-wall-repeated.png).

## Placeholder performance baseline

The baseline uses the current production build and a fresh-host visible Chromium/Wayland profile,
plus the managed LAN product at `http://192.168.1.22:37300/`. The machine is Arch Linux
7.0.11, Ryzen 7 8845HS / Radeon 780M, Chromium 148.0.7778.215, at a 1600×900 browser viewport.
`wayland-info` reported the active HDMI output at 2560×1440 and 59.951 Hz; EDID also advertises
119.989 and 144 Hz modes. The automated window therefore measured approximately 16.7 ms
animation-frame cadence. A human-observed 8.4 ms cadence on a 120 Hz mode is likewise expected
refresh synchronization, not 8.4 ms of renderer work.

### Content and startup

| Measurement                                              |               Current value |
| -------------------------------------------------------- | --------------------------: |
| Canonical project JSON                                   |               617,996 bytes |
| Project-referenced GLB plus license                      |               218,201 bytes |
| All files under `content/assets`                         |               220,725 bytes |
| Initial Angular JavaScript                               |           223,538 raw bytes |
| Lazy game JavaScript                                     |           901,389 raw bytes |
| All JavaScript                                           |         1,128,218 raw bytes |
| Cold usable menu                                         |                  105.656 ms |
| Warm usable menu                                         |                   66.296 ms |
| First authoritative projection and shared-renderer frame |                    494.4 ms |
| Session bootstrap / equivalent whole state               | 1,016,524 / 1,015,394 bytes |
| Maximum initial update build                             |                  140.195 ms |
| Static resource retransmissions during ordinary updates  |                           0 |

The fresh gameplay profile used 11,553,112 bytes of JavaScript heap and 1,001,410,560 bytes across
Chromium's entire browser/renderer/GPU/network/utility process tree. The latter is not a native
Loading Bay memory footprint. One 166 ms startup long task occurred. A two-second gameplay idle
sample accumulated 254.656 ms total task time and 124.260 ms script time with no event-listener
growth; fixed simulation/projection and the shared render loop were active.

### Five-second fresh-host input

| Measurement                             |    p50 |    p95 |    p99 |    max |
| --------------------------------------- | -----: | -----: | -----: | -----: |
| Input event to next frame (ms)          |    0.9 |    1.6 |    1.7 |    3.8 |
| Authoritative consumed-command RTT (ms) | 19.688 | 47.581 | 48.892 | 53.217 |
| Renderer cadence (ms)                   |   16.7 |   16.8 |   50.1 |   50.1 |
| Synchronous backend submission (ms)     |    0.4 |    0.7 |    1.2 |    1.2 |

### Twenty-second managed-LAN input

| Measurement                             |    p50 |    p95 |    p99 |    max |
| --------------------------------------- | -----: | -----: | -----: | -----: |
| Renderer cadence (ms)                   |   16.7 |   16.8 |   33.3 |   33.4 |
| Synchronous backend submission (ms)     |    0.4 |    0.7 |    0.9 |    1.0 |
| Snapshot cadence (ms)                   |   31.6 |   41.1 |   41.9 |   41.9 |
| Authoritative consumed-command RTT (ms) | 18.523 | 47.286 | 48.254 | 49.267 |
| Dynamic payload (bytes)                 |  2,124 |  2,393 |  2,739 |  2,752 |

The LAN run collected 193 unique renderer samples and 440 authoritative commands. It held 35
projected entities, eight resident voxel chunks, zero through three render diffs, at most two input
frames, one edge, and one outbound message. Dropped facts and runtime errors were zero.

The original #6351 evidence records draw and backend resource counts as unavailable because its
then-pinned public surface did not publish them. That historical baseline is not retroactively
filled from authored entities: visibility, materials, viewmodels, animation, and backend batching
can all change the real answer.

Rusty Engine #6361 and downstream #6378 now close that observability gap at exact Engine revision
`a6857d03141e162511231c276ee751a3413c90e5` and Demo implementation revision
`602e8ed60312aaea308097abb9816b8523a5bd1f`. The shared surface publishes immutable typed
statistics, and the ordinary desktop/headed tools retain the complete status, scope, and value.
The exact browser proof records this current placeholder and a deterministic richer stress load:

| Renderer statistic  | Scope          | Placeholder | Rich stress | Delta | Restored |
| ------------------- | -------------- | ----------: | ----------: | ----: | -------: |
| Draw calls          | per submission |          39 |          71 |   +32 |       39 |
| Live render handles | live resident  |          51 |          84 |   +33 |       51 |
| Geometry resources  | live resident  |          43 |          47 |    +4 |       43 |
| Material resources  | live resident  |          55 |          59 |    +4 |       55 |
| Texture resources   | live resident  |           0 |           0 |     0 |        0 |
| Animated instances  | live resident  |           0 |           0 |     0 |        0 |
| Triangles           | per submission |      14,380 |      14,444 |   +64 |   14,380 |

All values above had `available` status. The zeros are therefore exact observations, not missing
data. The rich sample is a 32-instance/four-shared-asset renderer-neutral stress overlay on the real
Loading Bay surface. It proves counter sensitivity, resource sharing, and exact cleanup; it is not
a substitute for measuring the final authored VC9 scene. Structured evidence is in
[`docs/evidence/renderer-statistics-certification.json`](evidence/renderer-statistics-certification.json).

## Budgets and comparison rules

The existing interactive budgets in `docs/performance.md` remain the pass/fail floor:

- renderer cadence p95 ≤ 20 ms and p99 ≤ 33.5 ms on the supported profile;
- local authoritative command RTT p95 ≤ 50 ms and maximum ≤ 100 ms;
- dynamic payload p95 ≤ 4 KiB;
- at most one in-flight plus one coalesced continuous input frame;
- at most 32 edge commands, one outbound message, and zero dropped facts.

VC9 must compare the placeholder baseline, brush proof room, and complete content-rich game without
changing the route or substituting a reduced test scene. The #6361 public counters are now
available, so VC9 establishes explicit budgets for renderer-owned draw/resource counts, animated
instances, voxel meshing, memory, cold/warm Studio open, save/reload, reset, and disposal from the
real authored workloads.

Content byte growth is reported but is not itself a desktop failure. Startup, parse/decode,
resource upload, memory, missed refresh intervals, submission duration, and input response decide
whether richer assets create a performance problem.

## Reproduction

From a clean checkout at the exact revision:

```bash
pnpm install --frozen-lockfile
pnpm run check:content
pnpm run build:shell
pnpm run test:browser
pnpm run profile:desktop
RUSTY_ENGINE_DEMO_URL=http://192.168.1.22:37300/ pnpm run certify:performance
wayland-info
```

Inventory checks:

```bash
jq '.assets' content/projects/loading-bay.project.json
jq -r '.scenes[0].entities[] | select(.renderable != null) |
  [.id, .name, .renderable.asset, .renderable.visible] | @tsv' \
  content/projects/loading-bay.project.json
curl -fsS http://192.168.1.22:37300/api/state | jq \
  '{projectionCount:(.projection|length), voxelChunkCount:(.voxelMeshes|length)}'
```

VC6 brush-kit source and canonical evidence checks:

```bash
blender --background --python scripts/blender/build-loading-bay-brush-kit.py
node scripts/check-brush-kit.mjs
pnpm run check:content
```

`scripts/author-brush-kit.mjs` is the explicit Studio mutation recipe used to create the canonical
artifact. Run it against a disposable copy of the pre-VC6 project when reproducing authoring
timings; it intentionally publishes nine assets and 25 instances and therefore is not part of the
ordinary read-only content check. `scripts/capture-brush-kit-studio.mjs` captures the supported
browser proof from an already-running Studio host selected by `RUSTY_STUDIO_BRUSH_HOST`.

The managed URL must identify `rusty-engine-demo` from `/health` before certification. Headless
SwiftShader browser smoke remains lifecycle proof, not hardware performance evidence.

## Review checklist

- [ ] No imported or generated binary asset entered #6351.
- [ ] Every current visual identity, environment, viewmodel, and invisible proxy has a
      classification and follow-on owner.
- [ ] The canonical project migration removes generator overwrite risk without introducing a
      second scene authority.
- [ ] Every shortlisted external source has author, version, license, local path, and hash.
- [ ] Detailed visual assets remain separate from explicit gameplay proxies.
- [ ] Object-local fine voxels are reusable mesh-authoring assets, not a unified world grid.
- [ ] Placeholder and final measurements use the same route, viewport, commands, and metric
      meanings.
- [ ] Frame cadence is never described as GPU/render duration.
- [ ] Renderer-owned counters come from the associated immutable shared-surface submission, never
      authored-state inference or WebGL instrumentation.
- [ ] No Doom map, texture, mesh, sound, name, or other licensed content is copied; only general
      compact-FPS readability and industrial-detail vocabulary informs the original design.
