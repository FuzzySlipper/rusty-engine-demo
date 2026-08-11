# Source provenance

Rusty Engine Demo was originally extracted from
[`FuzzySlipper/rusty-engine`](https://github.com/FuzzySlipper/rusty-engine) at commit
`a2e55f9660e46751d4c78bcdd23b9a321b0dc961` under Den task #6137.

The current Rust facade resolves directly from the adjacent `../rusty-engine` checkout.
This repository does not record or manage an active Engine revision. Exact revisions below are
historical extraction, feature-provider, and review provenance only; they are not dependency
selections or freshness requirements.

## Task #6703 facade and renderer-boundary migration

The previous active provider revision was
`5019ade33994bba02e8f0f7112fdfd8cd7e0c730`. Task #6703 advanced to
`d0b5e672b83d463bff71d8d35c877f770142ff3c`, replaced the downstream selective Engine crate list
with one namespace-preserving `rusty-engine` facade dependency, and removed the browser product's
direct renderer packages and TypeScript renderer assembly.

The checked-in `content/assets/brush-kit/vent-panel.glb` now also supplies the concrete native-host
resource proof. Loading Bay Rust admits the canonical project, maps Engine-reported physical input
to the player controller, maps an Engine pick with entity provenance to the authored generator
interlock and applies its game-owned activation consequence, then round-trips the resulting game
snapshot. Engine owns the private renderer artifact, Rust/TypeScript decoder boundary,
retained-resource lifecycle, and transactional cleanup. The four root renderer package entries are
exact dev-only resolvers for Engine Studio's published peers; they are not imported by downstream
source or exposed as a game renderer surface.

That revision also supplies the reviewed `gameplay-mechanics` provider adopted by Den task #6290.
Loading Bay uses its registered component store and named inventory, equipment, track, effect, and
damage services for canonical live quantities and mutations. Loading Bay retains item/weapon
meanings, fixed-tick ordering, ammunition and cooldown policy, pickups, combat targeting, death
consequences, saves, projections, and schema migration; no sibling checkout or TypeScript
mechanics authority is required.

## M10A Rust transfer

| Local surface                             | Source path             | Treatment                                                                                                                      |
| ----------------------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `rust/crates/loading-bay-game`            | `rust/crates/game-host` | Copied as one cohesive gameplay vertical; package/crate imports renamed from `game-host`/`game_host`.                          |
| `content/projects`                        | `content/projects`      | Copied unchanged for loading-bay and converted-content admission/product behavior.                                             |
| `content/generated`                       | `content/generated`     | Copied unchanged for migration, encounter, controller, navigation, and workload tests.                                         |
| `content/assets/kenney-wall-a.voxel.json` | same path               | Copied as the canonical converted-asset test input; later re-encoded through the current Engine voxel owner as recorded below. |

Reusable Rust crates are not copied. Cargo consumes the complete `rusty-engine` facade directly
from the adjacent `/home/dev/rusty-engine` checkout through one unconditional path dependency. The
downstream repository does not own an Engine version, pin, SHA, freshness check, or update helper.

The original Engine repository contains the historical Asha donor provenance for low-level code.
This repository records its immediate Engine source and does not recreate the old donor hierarchy or
runtime claims.

## M10B browser transfer

| Local surface                                 | Source path | Treatment                                                                                                                                                                  |
| --------------------------------------------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ts/packages/project-content`                 | same path   | Copied as the immutable fixture composer and schema/assertion package, then narrowed under #6353 so it cannot reproduce or overwrite canonical Studio project scenes.      |
| `ts/packages/browser-shell`                   | same path   | Copied as the product-owned input, projection, feedback, and browser shell; imports renamed to the demo package scope.                                                     |
| `ts/packages/render-contracts`                | same path   | Initially copied into the demo; removed under #6162 after its complete successor became a shared exact-revision Engine package.                                            |
| `ts/packages/renderer-three`                  | same path   | Initially copied into the demo; removed under #6162 after the retained Three/WebGL backend and browser surface moved behind shared Engine packages.                        |
| `scripts/browser-smoke.mjs`                   | same path   | Initially copied as an end-to-end product proof; retired after focused shell, native-host, E1M1 renderer, and desktop-host checks took ownership of its useful assertions. |
| Root pnpm, TypeScript, and Vite configuration | same paths  | Copied and narrowed to the demo-owned package identities and verification gate.                                                                                            |

The browser packages initially moved together because all four served one product at extraction
time. That intermediate direct renderer-package arrangement ended under task #6703. Only the
game-specific input, transport, semantic HUD/readout projection, and product shell remain here;
rendered product integration now crosses the Engine-owned Rust adapter.

Den task #6378 advances that complete package family to Engine
`a6857d03141e162511231c276ee751a3413c90e5` and consumes only the public immutable
`RendererSurface.submission()` sample for renderer statistics. The downstream proof adds no Three
import, WebGL inspection, private renderer object, resource cache, or second render loop.

Engine task #6406 extends the same public observation through
`StudioViewportComponent.frameSubmitted` and `StudioShellComponent.frameSubmitted`; task #6356
consumes the reviewed multi-pass correction at exact revision
`70808ba1b74b908c47edfbf3b1282fb2eb5f192d`. The Loading Bay recorder retains only immutable
bounded evidence and never acquires a surface, Three scene, WebGL context, resource cache, or frame
scheduler.

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

`content/projects/relay-annex.project.json` is likewise native downstream content. Under #6353 it
became a canonical Studio-owned serialized project admitted by the same Rust project path and
headless beacon proof; it was not transferred from Engine and no complete TypeScript scene copy
remains.

## M11B Studio adapter and voxel-owner adaptation

The project-owned Studio adapter was authored directly in this repository against the exact Engine
revision in [`engine-source.json`](../engine-source.json). It composes the public `asset-catalog`,
`authored-scene`, `content-store`, `entity-state`, `engine-inspector`, `render-model`, and
`render-projection` crates while retaining Loading Bay schema, layout, and domain admission here.
No Studio or adapter implementation was copied from Engine or Asha.

Protocol 11 adds the downstream half of Engine's bounded voxel-object placement seam. The adapter
resolves one already-authored object at exact project and object-content hashes, returns a
resource-only renderer frame for Studio's disposable ghost, and performs no project mutation or
retained preview-state change. `attachVoxelObjectInstance` remains the sole durable placement
operation and atomically publishes the Loading Bay entity owner plus instance before returning a
canonical reread.

That Engine revision also made the converted-voxel owner contract explicit. The checked-in
`kenney-wall-a.voxel.json` and its embedded project copy retain the same source mesh, sparse voxel
runs, material mapping, bounds, and provenance, but are re-encoded with the Engine-owned material
palette plus voxel-data and content hashes. This is a successor-codec adaptation of the existing
artifact, not new product content or a restored Asha dependency.

## M11F Studio parity adaptation

Protocol 6 was authored directly in this repository against Engine revision
`464dd5e16bb023ad8d81515eabeaac9bb75df74d`. It adapts the Engine-owned source-asset import,
catalog/lock, renderer payload, voxel primitive/template, history, annotation, conversion, canonical
asset, and environment-authoring mechanisms into named Loading Bay project operations. Trusted
host-file publication, source-drift inspection, and private prepared-candidate state are downstream
adapter responsibilities. No Asha replay, facade, generic command, Studio, or demo topology was
transferred.

## M11G Studio voxel-object and flipbook adaptation

Protocol 7 was authored directly in this repository against the Engine voxel-object Studio provider
landed at `f725962bce3e18ffdf202086e00e5b111ce31823` and consumed through exact protocol-7 harness
descendant `1ee4af531c848e3931cce33b22dc63405d48e3e7`. The adapter composes Engine-owned static and animated
GLB inspection, bounded conversion planning, private candidate preview, exact output application,
voxel-object runtime admission, and retained renderer projection. Loading Bay owns only its explicit
project schema, catalog references, scene attachment operation, atomic publication, and typed
protocol composition.

The static proof reuses the already-recorded CC0 `kenney-wall-a.glb` fixture. The animated flipbook
proof reuses the already-recorded CC0
`content/assets/kenney-retro-character-medium.glb`; task #6257 introduced no new third-party source
or generated binary asset. Persisted conversion provenance retains the exact source path, SHA-256,
byte count, converter/settings hash, optional license path, and animated source-clip schedules.

The animated appearance proof uses Kenney's CC0 `Animated Characters Retro` medium character as a
checked-in product asset rather than an Engine fixture:

- `content/assets/kenney-retro-character-medium.glb`: 217,536 bytes, SHA-256
  `c71255a41c0373f0d2ef52593369d5fd9d2f6220ae548aff8cd6bf5edb403674`
- `content/assets/KENNEY-ANIMATED-CHARACTERS-RETRO-LICENSE.txt`: 665 bytes, SHA-256
  `d344fd83cc72bedadecbf2d051b904d3e63378cb87b489122a3efdb850b7ca7c`

The project catalog admits that exact content hash and its named `idle`, `run`, and `jump` clips;
Studio resolves the bytes through its bounded trusted-host resource path and the shared renderer.

## M11H Studio applied-object ownership and playback adaptation

Protocol 9 was authored directly in this repository against reviewed Engine provider revision
`ff87c425be4167a5bdd06c059b042967f2808e2b`. It composes Engine's closed protocol-9 client,
`VoxelObjectPlayer`, and retained voxel-object projector into Loading Bay's explicit entity-owned
project schema. Loading Bay owns entity allocation, ownership admission, atomic persistence, and
lifecycle invalidation; Engine owns clip timing, loop semantics, playback posture, and incremental
render operations. No timer, durable gameplay state, alternate projector, or sibling-checkout
dependency was introduced. The proof reuses the already recorded CC0 wall and character assets and
adds no third-party source.

## Protocol-10 downstream Entity inspector composition

The protocol-10 component-reference vocabulary and static Studio inspector
outlet are consumed from the exact reviewed Rusty Engine provider selected by
Den task #6304. Loading Bay contributes only its original weapon configuration
form, closed v1 client/decoder, and Rust authoring service; it adds no third-party
art, code, generated binary asset, generic component schema, or runtime-loaded
extension.

`apps/loading-bay-studio/src/theme.css` retains the stock Engine Studio custom
property names and values so the explicitly composed shared shell has the same
host chrome in its downstream application. The application bootstrap and
startup selector were independently authored against the public Engine
composition API; no Engine app TypeScript, private workspace store, host
script, or sibling checkout is copied into the supported product.

## Original VC6 voxel-brush kit

The nine Loading Bay brush sources under `content/assets/brush-kit` are original procedural
geometry authored for Den task #6356 by
`scripts/blender/build-loading-bay-brush-kit.py`. No external mesh, texture, palette, map, or game
asset was read, copied, traced, or converted, so there is no third-party license notice for this
kit. The source script's SHA-256 is
`773b936bf0d0ebe5beb5d09264b4cfab7a9b933216f708d7922e393624e65fdc`; its generated source
manifest is `8816e0e6ba66a9b35ea26169f6d75b25fdca818c567e6ce7023fd8088654ae5`.

| Original source GLB     | Bytes | SHA-256                                                            |
| ----------------------- | ----: | ------------------------------------------------------------------ |
| `wall-conservative.glb` | 2,276 | `ce4c736ce80c0210c77098fbcb1f900f3119f196c8a97a34a97d5455aef89cca` |
| `wall-dense.glb`        | 5,140 | `6dca2d9103849789bc596550f069ced651456c83df28a09e1f5642b146e9ce04` |
| `corner.glb`            | 2,004 | `068b492c3ad7605d87e3fb9c372e45f4ad284b5e673cc7bfbe9e26af2a7e03cb` |
| `doorway.glb`           | 2,248 | `933775b7a53c9aa37c888234b658c5c897b45794ed5f78982617ddb91c2cdcae` |
| `vent-panel.glb`        | 2,980 | `0f270ff6d512b3c1c4a9b8c1559d3673bc910fe90ede8ae4aa87fc2ae3442fad` |
| `column.glb`            | 2,004 | `773073a4aea11a33bf85a5aa32b8211ac5337ab2ae5d2a664edd46545fdb19ca` |
| `floor-strip.glb`       | 2,500 | `da9f469ff48955db631d7b693632c4b0511fad7497d65e0ba6871e7fb9e8cdb6` |
| `ceiling-strip.glb`     | 2,028 | `519308222335856fb05de4012daa17daca2452273cb49b1900543443bd558ffd` |
| `landmark-relay.glb`    | 3,952 | `4592d5c7ddbac4355db126651cab0cfa454068b21be2b967bd00f9d815a19571` |

The matching `.mesh.json` files are deterministic authoring intermediates for Studio source import,
not an alternate retained or runtime format. Blender 5.1.2 on the authoring host could construct
the meshes but its bundled glTF add-on could not load because that Blender installation lacks
NumPy. The committed script therefore writes the small standards-compliant GLBs directly from the
same Blender-created positions and indices. Studio then inspects those GLBs and owns conversion to
the canonical sparse voxel-object definitions embedded in
`content/projects/loading-bay.project.json`.

All accepted definitions preserve exact source paths, byte counts, and SHA-256 values in their
canonical conversion provenance. `scripts/check-brush-kit.mjs` verifies those values against disk,
the canonical sparse-run counts, 25 shared-definition instances, decorative proxy separation,
fresh-process Studio proof, and screenshots.

## Original VC7 Loading Bay brush composition

Den task #6357 adds no external source asset. Its current canonical descendant retains all nine
original VC6 definitions and uses eight of them through 340 entity-owned route instances authored by
`scripts/author-loading-bay-brush-level.mjs`. The placement recipe reads only the canonical
Loading Bay project: floor, ceiling, and wall coverage comes from the explicit hidden gameplay
proxy; doorway jambs and overhead headers derive from the five Rust-owned door entities; columns,
corners, and two relay landmarks use original Loading Bay positions.

The recipe publishes 11 bounded protocol-12 batches through the real Studio adapter. Floor and
ceiling coverage uses deterministic rectangles no larger than 8 by 8 world cells, retaining
independent serialized owners and picking while avoiding hundreds of redundant draw objects. Exact
ordered receipts, 340 route-owner identities, per-definition repeat counts, canonical reread,
fresh-adapter reconstruction, and the 772,551-byte structural projection are recorded in
`docs/evidence/voxel-level-brush-authoring.json`. `scripts/check-brush-level.mjs` ties those facts to
the serialized project and verifies that every new owner is decorative. No Doom map, texture,
mesh, palette, layout data, or other third-party content was copied or traced; Doom E1M1 remains
only the previously documented high-level compact-FPS readability reference.

Task #6473 is a source-free canonical descendant of that composition. It adds 14 Rust-authored
material voxels at six existing column locations, removes two now-redundant wall presentation
owners, realigns the southern corners, and retains the nine original definition sources unchanged.
Each doorway uses collision-backed wall instances as jambs and a conservative-wall header whose
occupied cells remain above walk height. The global audit derives all walk-height occupied
intervals from sparse runs and requires collision coverage across the whole material-voxel row, so
one doorway cannot intrude into another opening unnoticed. Its exact current 340-placement receipt
and 772,551-byte structural projection remain in
`voxel-level-brush-authoring.json`; the complete occupied-surface comparison is in
`wall-proxy-alignment.json`.

## VC4 production animated actor source kit

Den task #6355 replaces the historical three-clip appearance proof with two production-style actor
sources built reproducibly from Kenney's CC0 1.0 `Animated Characters Retro` pack. The authoritative
recipe is `scripts/blender/build-loading-bay-actor-library.py`; it was run with Blender 5.1.2
(`ec6e62d40fa9`) using:

```sh
PYTHONPATH=/usr/lib/python3.14/site-packages blender --background --factory-startup \
  --python scripts/blender/build-loading-bay-actor-library.py -- \
  --source-root /home/stash/mesh-resources/kenney_animated-characters-retro \
  --output-dir content/assets/actor-kit
```

The recipe imports the medium rig once per variant, normalizes the armature and skinned mesh
together to 1.78 world units, retargets Kenney's idle/run animation-only FBXs as local rotation
deltas onto the model FBX bind skeleton, installs one embedded nearest-filtered skin, and exports
one mesh, one armature, one material, and six named actions. Imported non-root joint translations
are deliberately discarded: their animation-FBX rest poses differ from the model bind pose and
were the source of the visible stretched/flopping limbs. Jump, attack, hit, and death are explicit
Loading Bay whole-body actions keyed as local rotations on fourteen existing torso, head, arm,
and leg joints. No action changes a bind bone length. Every sampled non-jump pose is normalized to
the authored contact plane; jump alone retains its intentional 0.38-unit arc. These
recipe-defined derivatives do not rename or misrepresent stock clips and carry no gameplay
authority.

| Product source                            | Skin                |   Bytes | SHA-256                                                            |
| ----------------------------------------- | ------------------- | ------: | ------------------------------------------------------------------ |
| `content/assets/actor-kit/arc-warden.glb` | `zombieMaleA.png`   | 353,956 | `a1069d4bfa950aeade3ae032291279684014c1cf93e12fd98359555a9fc259e1` |
| `content/assets/actor-kit/bay-rusher.glb` | `zombieFemaleA.png` | 348,372 | `e7e5c7a3a79abac6b24b0ec511b61fd75f82d900cdc4af9fb526278c5f5033f7` |

Both outputs contain exact `idle`, `run`, `jump`, `attack`, `hit`, and `death` clips. Their complete
source file hashes, clip ranges/durations/origins, Blender version, target scale, final bytes, and
asset identities are closed by `content/assets/actor-kit/source-manifest.json`.
`scripts/check-actor-kit.mjs` independently parses the shipped GLB JSON and binary chunks and
rejects hash, size, clip, mesh, skin, material, embedded-image, or external-buffer drift. For each
attack and hit clip it also decodes the exported quaternion accessors and requires nontrivial
rotation deltas on at least eight reviewed skin joints, excluding whole-armature motion as
sufficient animation evidence. It verifies each manifest duration against the shipped glTF time
accessors. The generator also
factory-resets Blender and reimports each finished GLB before recording its manifest, so a claimed
clip cannot exist only in the `.blend` session. A second factory-startup invocation produced
byte-identical GLBs at both recorded hashes.

`content/assets/actor-kit/KENNEY-CC0-LICENSE.txt` preserves the source notice wording while
normalizing its indentation, trailing whitespace, and line endings to repository text conventions
(642 bytes, SHA-256
`6d4444c863076faaf18c4a2c279ad1cf45b91cef1f4db3247a312ad6827298cc`). The original source
`License.txt` hash remains recorded separately in the manifest.

Rusty Engine tasks #6538/#6546 supply the human-visible Tools > Animation Inspection workflow,
fail-atomic skinning facts, exact shared-viewport projection, and bounded contact-sheet framing at
public revision `d52c9b0f3287f21eea81d465871978a117750d0c`. The Rust-owned Studio path publishes the two assets
into schema-24 project hash
`6069198b7bac3792ff86c0e245a4cdcae72ae1c68bf90f42cb08c39bc656c328`,
preserving each source path, SHA-256, byte count, converter/settings hash, license path, bounds, and
six source-clip schedules. No-op reimport preserves that hash; changed source requires explicit
reimport; stale hash, duplicate identity, malformed GLB, missing external texture, duplicate clip,
and oversized input failures are typed and non-mutating.

The supported Studio browser proof opens the canonical project through the visible Studio shell,
selects the two real actor entities, opens Tools > Animation Inspection, and captures all twelve
actor/clip sheets at labeled 0/25/50/75/100% times. The same workflow exposes the 58-joint
hierarchy, finite inverse binds, normalized weights with zero invalid rows, interpolation modes,
independent root/skeleton clone identity, and shared render resources. Independent Blender 5.1.2
dependency-graph samples of the admitted GLBs at those exact times match Engine sampled bounds to
at most 0.000035 units and exact vertex counts. The Blender evidence imports no Engine or Three.js;
its three human-readable montages and full sample manifest are stored beside the Studio sheets.
The public shell submission proof also uses two identities and 12
temporary instances spanning every clip. It records exact resident deltas of +2 geometries, +2
materials, +2 textures, and +12 animated instances, plus owner-ordered fresh-adapter reconstruction,
resize, project close/open, cache-bypassing page reload, and renderer disposal. The checked GLBs
remain source assets rather than a downstream hand-written render model or private Three.js loading
path. The exact Blender sample ledger and montages are in
`docs/evidence/animated-mesh-contact-sheets/blender-source-baseline.json`; the public Studio capture
receipt is `docs/evidence/animated-mesh-contact-sheets/certification.json`; and the checked
cross-sampler/lifecycle comparison is
`docs/evidence/animated-mesh-contact-sheets/source-equivalence.json`. Import and retained-resource
evidence remains in `docs/evidence/actor-kit-authoring.json` and
`docs/evidence/actor-kit-studio-browser.json`.

## VC5 serialized industrial prop kit

Den task #6354 replaces the former non-actor primitive presentation with canonical Studio-imported
static meshes. The exact derivative/source/bounds/material record is
`content/assets/prop-kit/source-manifest.json`, and the exact Studio import, appearance mapping,
no-op reimport, canonical reread, and fresh-process reconstruction receipt is
`docs/evidence/prop-kit-authoring.json`.

Eight mesh sources are copied from the local Kenney packs and retained with their unmodified
source hashes:

| Product asset                     | Kenney source                   | SHA-256                                                            |
| --------------------------------- | ------------------------------- | ------------------------------------------------------------------ |
| `mesh/prop-kit/security-door`     | Factory `door-wide-closed.glb`  | `987837d7b45bca466c5f2268fa0193e014374cc0d2ef0784f6c37092856b71d0` |
| `mesh/prop-kit/control-panel`     | Factory `screen-panel-wide.glb` | `092066a198f1f81c857ceab23497dd3ea6066d261a91497cf0f83f7ba951c4d0` |
| `mesh/prop-kit/hazard-marker`     | Factory `button-floor-square`   | `f32def1dd9a57939b096d64361fc5058a8ba240a0394951e8681fb7326ebdeb6` |
| `mesh/prop-kit/extraction-beacon` | Factory `scanner-high.glb`      | `b71d8ab86fe1a12542eac14e4185e17babf15ac086c3343acf8600a080ed985a` |
| `mesh/prop-kit/level-exit`        | Factory `indicator-special`     | `3cd514de3e283705df2baccf7cea62a70ba64e87311252fe26788be4377c0d49` |
| `mesh/prop-kit/status-runner`     | Factory `scanner-low.glb`       | `0baa337bc3c653522ce01baa09cc02b8810640225e823fe735dc8e4167a7913b` |
| `mesh/prop-kit/landmark-crane`    | Factory `crane.glb`             | `ceaf20fb976ce0415d2b3d40e723b6ad377845818fa4fa68b55434108b1a6880` |
| `mesh/prop-kit/landmark-tank`     | Industrial `detail-tank.glb`    | `b1edc2953c590c16f1d8280dfeb9073af6c710ac3d587f3dba144f69363d799b` |

The copied Factory Kit and City Kit Industrial notices are CC0 1.0, with SHA-256 values
`61e86565dd297e143ad631594980eda0a17fc81a4cd7c6d71acf2f5e0cad30b6` and
`bf1195a387c996ab4bb6d05bb7ead8c5b233c0532634fec916ef9e090936c3e5`.
`scripts/build-loading-bay-prop-kit.mjs` directly extracts bounded GLB geometry because the
installed Blender 5.1.2 glTF add-on cannot load without NumPy. It also deterministically authors
the nine original Loading Bay derivatives for energy cells, scatter shells, med patches, impact
vests, maintenance passes, three weapons, and the muzzle flash. Those original shapes, palette
materials, and names do not derive from Doom or another game.

Every imported static mesh declares `visualOnly`. Existing Rust entity bounds, collision,
kinematic, trigger, hazard, pickup, door, and progression components remain the sole gameplay
authority. The two new crane/tank landmarks intentionally have none of those components.
`scripts/check-prop-kit.mjs` verifies the copied notices, source and derivative hashes, admitted
bounds/material topology, Studio import provenance, exact mappings, viewmodel resources, and
visual/gameplay proxy separation.

The converted-wall browser fixture does not embed a second generated scene or presentation
fallback. After its schema-11 source is migrated by the Rust project store, the same
`scripts/author-prop-kit.mjs` Studio protocol path imports the security-door, control-panel, and
status-runner sources and publishes the five fixture appearances before launch. Those temporary
proof-project mutations reuse the manifest hashes and copied CC0 notices above.

## Physics projectile consumer

Den task #6580 adds the original `weapon/kinetic-launcher` and `ammo/kinetic-slug` definitions to
exercise the Engine rigid-body provider requested by Engine task #6535. The launcher reuses the
existing original `mesh/prop-kit/rivet-carbine` silhouette as a disposable viewmodel reference;
no new mesh, texture, sound, or third-party asset was introduced. Its authored Rust policy is
bounded mass/radius/impulse/gravity/lifetime/restitution plus the existing Loading Bay damage,
cooldown, ammunition, and muzzle-offset fields.

The consumer pins Engine `5019ade33994bba02e8f0f7112fdfd8cd7e0c730`, whose public
`engine-spatial::RigidBodyService` owns integration, contact generation, and rigid-body state
publication. Loading Bay's `ProjectileService` owns only projectile entity admission, initial
impulse requests, target/damage-once policy, impact/expiry facts, and snapshot stripping. The fixed
Loading Bay game loop is the sole step caller; TypeScript receives an immutable projectile node and
cannot advance or mutate the body.

The focused downstream proof is `tests/projectile_runtime.rs`: it selects the canonical launcher,
fires it through `GameRuntime::attack`, observes the real Engine motion receipt and changed
projection, then confirms snapshot/reopen retains the weapon definition and ammunition state while
omitting the transient projectile. This is a consumer proof, not an Engine fixture or synthetic
physics implementation. The canonical project JSON and its existing asset hashes remain otherwise
unchanged.

## Proper FPS campaign design

The product architecture, protocol targets, content vocabulary, and original level route in
`docs/fps-product-architecture.md` were authored directly in this repository under Den task #6215.
They are not transferred Doom content and do not copy another game's code, map, geometry, names,
sounds, textures, sprites, or trade dress.

The schema-13 Loading Bay and Relay Annex pickup objects were authored directly in this repository
under Den task #6221. Their `mesh/pickup-*` asset identities select game-specific colored primitive
presentation generated by the downstream projection adapter; they do not refer to copied mesh,
texture, sound, or sprite files. Rust owns their item/quantity configuration, trigger-driven
collection, atomic inventory grant plus entity consumption, snapshot state, facts, and rejection
identity. Rusty Engine contributes only the exact-pinned generic entity bounds and trigger-volume
mechanism.

The schema-14 weapon-item definitions, authored numeric slots, starter scatter-shell grant, and
browser inventory projection were authored directly in this repository under Den task #6222. They
reuse the existing downstream primitive pickup presentation identities and do not add copied mesh,
texture, sound, sprite, code, or level data. The `arc-pistol`, `breach-scattergun`, and
`rivet-carbine` identities and their combat values are original Loading Bay demo content.

The schema-15 player vitality, armor absorption, hazard objects, damage/death facts, authored
restart baseline, and health/armor HUD were authored directly in this repository under Den task
#6223. The Loading Bay and Relay Annex coolant leaks use the downstream semantic
`mesh/hazard-pad` primitive identity; they add no copied mesh, texture, sound, sprite, code, or
level geometry. Rusty Engine contributes only its exact-pinned generic entity-bounds and
trigger-volume mechanisms; Loading Bay Rust exclusively owns hazard cadence, damage, vitality,
death, restart receipts, snapshots, and browser projection.

The schema-16 single-shot, bounded deterministic spread, and held-automatic weapon behavior,
the rivet-carbine pickup, per-weapon shot feedback, and dry-fire feedback were authored directly in
this repository under Den task #6224. They reuse the existing downstream primitive pickup and
feedback-sprite identities and add no copied mesh, texture, sound, sprite, code, or level data. The
sidearm, spread, automatic, and dry-fire sounds are synthesized at runtime from the original
frequency/envelope profiles in `ts/packages/browser-shell/src/presentation-feedback.ts`; there are
no imported audio files. Rust exclusively owns selection, cadence, deterministic spread seeds and
rays, occlusion, ammunition, damage, and facts. Browser pulses, particles, synthesized audio, and
the shared-renderer viewmodel are disposable presentation and cannot alter aim or damage. The three
viewmodel silhouettes are original arrangements of renderer-neutral cube and sphere primitives
authored in `weapon-viewmodel.ts`; they contain no imported model, texture, sprite, animation, or
other asset. Bob, recoil, and muzzle flash are bounded local descriptor offsets derived from
accepted Rust movement and attack facts.

The schema-17 key-gated door, Loading Bay interlock, secret region, level exit, and their original
presentation strings were authored directly in this repository under Den task #6226. They reuse
existing downstream primitive door, control-panel, key-pickup, and exit identities and add no
copied mesh, texture, sound, sprite, code, or level data. Rust exclusively owns key validation,
retain/consume policy, switch consequences, first-discovery state, level completion, facts, and
snapshot state; browser prompts and completion overlays are derived presentation.

The schema-18 `sentry-strike` melee and `sentry-pulse` ranged combat configurations were authored
directly in this repository under Den task #6227. They reuse the existing original Loading Bay
sentry primitive meshes and synthesized downstream feedback profiles and add no copied mesh,
texture, sound, sprite, code, or level data. Rust exclusively owns bounded sight/hearing
activation, pursuit intent, canonical voxel occlusion, cadence, attacks, player damage/death,
facts, and snapshot state. Browser alert/attack/miss/damage cues and posture labels are disposable
presentation and cannot select targets or alter damage.

The schema-19 Bay Rusher and Arc Warden archetype identities, distinct primitive silhouettes,
bounded encounter activation, deterministic defeat-drop relationships, and drop/activation
feedback were authored directly in this repository under Den task #6228. They reuse existing
downstream primitive geometry, pickup identities, feedback sprites, and synthesized audio; they
add no copied mesh, texture, sound, sprite, code, or level data. Rust exclusively owns dormant,
active, and cleared encounter state plus exact-once pickup materialization and snapshot state.
Browser silhouette materials, particles, billboards, synthesized audio, and posture labels remain
disposable presentation.

The complete schema-19 Loading Bay campaign and its Relay Annex data-only variation were authored
directly in this repository under Den task #6229. Den task #6353 migrated their complete source of
truth from the former TypeScript composition to the canonical serialized files in
`content/projects`; `pnpm run generate:content` now materializes only deliberate fixtures under
`content/generated`. The floor plan, 3,931 material-voxel arrangement, room proportions, door and
encounter placement, route, lighting, object names, combat values, and progression text are
original Loading Bay content. Doom was used only as a high-level reference for the familiar
vocabulary of a compact key/switch/secret/weapon-upgrade FPS route. No Doom source code, map data,
geometry, node/blockmap data, textures, flats, sprites, sounds, music, names, story text, or trade
dress was read, converted, copied, or distributed.

The original authored placeholder identities described above remain historical introduction
points. Under #6354 every visible non-actor gameplay entity instead references the serialized
prop-kit assets recorded in the preceding section. Under #6355 and #6358 all eight visible enemies
use the serialized Arc Warden or Bay Rusher animated actor with a capability-complete posture
binding. The first-person player marker and gameplay proxies are intentionally not rendered; no
shipped visible renderable uses a primitive fallback. All checked-in third-party visual sources are
the separately itemized Kenney CC0 files in this document.
Campaign route checkpoints, deterministic artifact hashes, and product proof are recorded in
`docs/loading-bay-playtest.md`.

The agent-facing extension recipes and executable boundary checks added under Den task #6232 are
native repository documentation and verification code. Their item, weapon, enemy, and layout
locality examples reference the already-recorded Loading Bay and Relay Annex compositions; they do
not add or transform any visual, audio, map, or third-party asset.

The public `/home/dev/rusty-engine-ui` checkout at exact commit
`68ddfa5430ec3bc2cf7ca96963982db9511e79ba` supplied the following #6216 downstream shell patterns:

| Local surface                                                                                     | Donor surface                                                                                                       | Treatment                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Root Angular 21 / Nx 23 workspace, boundary tags, strict app TypeScript, and pnpm build allowlist | root `package.json`, `pnpm-workspace.yaml`, `nx.json`, `boundaries.json`, `eslint.config.mjs`, `tsconfig.base.json` | Selectively adapted around this repository's existing Rust and `ts/packages/project-content` owners; package names, scopes, targets, and paths are Loading Bay-specific.                                                                                                       |
| `libs/theme`                                                                                      | `libs/theme`                                                                                                        | Palette and reusable panel-token structure adapted to the existing Loading Bay visual language.                                                                                                                                                                                |
| `libs/platform`                                                                                   | `libs/platform`                                                                                                     | Narrow `DocumentEffectsPort` and browser implementation selected; unused storage, clock, and clipboard ports were not copied.                                                                                                                                                  |
| `libs/ui-compass`                                                                                 | `libs/ui-compass`                                                                                                   | Pure presentation algorithm and component structure adapted to the full-viewport FPS overlay.                                                                                                                                                                                  |
| `libs/ui-combat-log`                                                                              | `libs/ui-combat-log`                                                                                                | Pure presentation structure adapted to committed Rust fact projections.                                                                                                                                                                                                        |
| `apps/loading-bay`                                                                                | `apps/app` foundation                                                                                               | Angular bootstrap, hash-router ownership, and standalone-component patterns adopted; the actual viewport, HUD, diagnostics, input, renderer, and runtime projection remain this game's implementation.                                                                         |
| `libs/ui-game-panels` and #6225 route panels                                                      | donor hotbar, inventory, menu, and settings presentation patterns                                                   | Pure component structure and visual vocabulary selectively adapted. Loading Bay rewrites every input around immutable Rust projection, typed session commands, and fail-safe host-user preferences; donor placeholder actions and UI-owned inventory mutation remain excluded. |

No donor feature screen, fake transport, demo configuration, placeholder action provider, store
kernel, UI-owned inventory/equipment state, or inert menu control was imported. The migrated route
mounts the exact shared Engine renderer already used here and disposes its input listeners, held
input, presentation feedback, and shared surface through one route-owned lifecycle.

Task #6225 keeps that restriction while making the shell a usable game surface. New Game,
Continue, pause, item use, interaction, and weapon selection invoke the existing typed game-session
contract; the menu and panels never retain mutable gameplay state. Mouse sensitivity, invert-Y,
effects volume, HUD visibility, telemetry visibility, and the continue marker are browser-host
preferences only. The desktop and narrow-viewport product proof drives the real Rust host and
shared renderer rather than donor fixtures or a fake transport.

Rusty Engine task #6213 produced the renderer-owned timing seam at public SHA
`2665b74566136fb77e3a26b0766394124c8f58d3`. That SHA is recorded here as reviewed-upstream
integration evidence adopted by #6219. Rusty Engine task #6263 then produced the bounded
camera-relative `viewmodel` layer and explicit world/depth-clear/viewmodel composition at public SHA
`e622c941671bc0f167206b049ab94ea63495a86d`, initially adopted by #6224 and retained in the current
reviewed Engine descendant `0e0c49442d0c3d876a1336a5a829087f6e2314db`. Engine task #6416 owns
the correction which keeps a serialized static-mesh definition reusable after its last live
instance is destroyed; #6354 consumes it for world-pickup to camera-relative viewmodel transitions
without a downstream resource cache or redefinition loop.
The downstream call site reads `surface.timing()` from the shared auto-started surface; no demo
frame scheduler, backend clock, renderer object access, or private renderer was introduced.

## Doom E1M1 voxel showcase — offline source, no runtime WAD (Den campaign #6674)

Task #6801's bounded single-player gameplay scope, authored coordinate transform, landmark
measurements, and current product deltas are recorded in
[`doom-e1m1-gameplay-ledger.md`](doom-e1m1-gameplay-ledger.md). This provenance section remains the
authority for source bytes, license boundaries, and derived assets.

Doom E1M1 “Hangar” is the durable textured-voxel proving ground. The
original WAD bytes are an **offline source** for an offline Node TS
forge; no `doom1.wad` is read at runtime, no TS gameplay authority is
added, and no generic bridge is introduced. Geometry, texture, Thing placement,
and the bounded E1M1 gameplay semantics documented below are ported; no Doom
code, runtime map bytes, sprites, sounds, music, story text, or trade dress are
shipped.

### Source

- `doom1.wad` (IWAD, id Software shareware) at
  `/home/research/doom.ts/public/doom1.wad` — **4,196,020 bytes**,
  **SHA-256 `1d7d43be501e67d927e415e0b8f3e29c3bf33075e859721816f652a526cac771`**,
  identification `IWAD`, 1264 lumps. E1M1 is lump 6 (`E1M1`, 467
  vertices X −768..3808 Y −4864..−2048, 85 sectors floor −136..264,
  475 linedefs, 648 sidedefs, 138 Things). Only incidence is ported.
- Reference decoder `doom.ts` at `/home/research/doom.ts`
  (`src/doom/{wad,level,textures}`, GPL-3.0) is a **reading reference
  only**. The campaign’s TS decoder at
  `ts/packages/doom-e1m1-authoring/src/{wad-decode.ts,textures.ts,voxelize.ts}`
  re-implements `Wad`, `VertexArray`/`SectorArray`/`SideArray`/`ThingArray`,
  `Flat` (64×64 indexed via `PLAYPAL` 768-byte palette
  **SHA-256 `fd895921b5d0a394612bb29852ed003d44d69f76dec31c0dc6b5d5fc7d63f7bb`**)
  and `TextureArray` composite (`PNAMES`+`TEXTURE1`+ patch posts,
  case-insensitive) without line-copying GPL code. No `doom.ts` file is
  shipped.
- Additional shareware WAD extraction directory `public/doom1wad/E1M1.dat`
  is not consumed; the WAD itself is the hashable source.

### Derived textures (54 VTX6 PNGs, RGBA8 non-interlaced sRGB straight-alpha)

One PNG per distinct E1M1 incidence (32 walls, 22 flats) staged at
`content/doom-e1m1/textures/{flat,wall}/*.png` with `manifest.json`
(`wadSha256` above, `paletteSha256` above, `generatedAt`
`2026-08-07T00:00:00.000Z`). Budgets: each ≤16 MiB, total decoded RGBA
≤256 MiB, identities ≤256, `tileScale` flats `1/64`, walls `1/width`
(`1/height`), VTX repeat with Nearest/Repeat.

| Kind | Name     | W×H     | PNG SHA-256                                                        | Bytes | tileScale                       |
| ---- | -------- | ------- | ------------------------------------------------------------------ | ----- | ------------------------------- |
| flat | CEIL3_5  | 64x64   | `77c168d323d085f8cbf2086a7c5929659d947c501f417a999b0a5a9d257f4dfb` | 1273  | 0.015625,0.015625               |
| flat | CEIL5_1  | 64x64   | `b1c0a3108ee78beecb11cfc2d8d5f20ccedf26354f92d96628f18b277ac5e45a` | 1098  | 0.015625,0.015625               |
| flat | CEIL5_2  | 64x64   | `b6746eff72cc3173498a3c0b51fca60f66539a75225c7ed69adfbbef8eebbd10` | 1915  | 0.015625,0.015625               |
| flat | F_SKY1   | 64x64   | `6afa2248c3bcaee3ce4c4714c6fc0f2c6e5cc445631ea4ea6a5ffe411eb3e7c6` | 3190  | 0.015625,0.015625               |
| flat | FLAT14   | 64x64   | `913214c542f492a7c5afd2768627ea66adb6ea92d9552c3e78e816c7cb106a78` | 1915  | 0.015625,0.015625               |
| flat | FLAT18   | 64x64   | `0304b158fe35b89e9bb34e7bd5f56b4c1284ffa53d829ed3bb425520b46d328a` | 1687  | 0.015625,0.015625               |
| flat | FLAT20   | 64x64   | `63a81f024b3f1e9dbd410e42c4ac84e68d7222227aa74fbed9daebc84246f29d` | 341   | 0.015625,0.015625               |
| flat | FLAT23   | 64x64   | `352bc3716d9728d297d5aa5a12d5dafc6dfe6863527b093b3ad21f234a53042c` | 976   | 0.015625,0.015625               |
| flat | FLAT5_5  | 64x64   | `225965d93b749707eab86b2c046369f015b112bbfc0e60cde54a23713af7a879` | 2707  | 0.015625,0.015625               |
| flat | FLOOR1_1 | 64x64   | `7276e451cbddc46874e000be70d259c0e25c638a0f7882e8bae6e99fcbe51d74` | 1826  | 0.015625,0.015625               |
| flat | FLOOR4_8 | 64x64   | `6d5f1445e69515774bbb78fc696aea0ef74fbb0d2124997fa676588d72e9f5c6` | 2215  | 0.015625,0.015625               |
| flat | FLOOR5_1 | 64x64   | `0bd25482a889c76e680ad9c02897621a5e9c7a6c0b1b1d0f944d667b35904e41` | 2582  | 0.015625,0.015625               |
| flat | FLOOR5_2 | 64x64   | `18df567753bd8082783706f7263f4922ddfab0505f03e1598e324782747c4fd7` | 1660  | 0.015625,0.015625               |
| flat | FLOOR6_2 | 64x64   | `72e18d2edd5fb2d2f5c1a7b73ec7982d5509246c491f30a4299a272884570b4a` | 2531  | 0.015625,0.015625               |
| flat | FLOOR7_1 | 64x64   | `2bb2b48229f67be9fc7f4ebd13e24bc9fed18148cd566284b041ea4d3bb0481a` | 2094  | 0.015625,0.015625               |
| flat | FLOOR7_2 | 64x64   | `64f9e17663049712b610402edba9904e6125f5387cb6fd97ece8ca16e31f5a2a` | 2094  | 0.015625,0.015625               |
| flat | NUKAGE3  | 64x64   | `ddf0636c3527c963d10b43a554138271ac62bdd5186d3960d3efb4a21bc9e4ca` | 1855  | 0.015625,0.015625               |
| flat | STEP2    | 64x64   | `e54d96aab7ded0b654ee7d0c43f108eed91cbe4fa430b3c8e7a9f992da027de2` | 1493  | 0.015625,0.015625               |
| flat | TLITE6_1 | 64x64   | `c4aa178c7949428c45e713a81ee00c75cad166ea0cc352ef75d941e1260a5d42` | 1177  | 0.015625,0.015625               |
| flat | TLITE6_4 | 64x64   | `30e4038a6874c014c45ba489b3b745570c42c5d188d038d3809deaa84b1e9b2a` | 1903  | 0.015625,0.015625               |
| flat | TLITE6_5 | 64x64   | `c9612267547d0e1a0c22d02a6e00497da8d34feee5a3ff8a22b98fd5c48fae31` | 1239  | 0.015625,0.015625               |
| flat | TLITE6_6 | 64x64   | `ffb2e7c9b9290c514789f6db8cd0e90fb8946f270f90d7a38e262968d8f2872f` | 1944  | 0.015625,0.015625               |
| wall | BIGDOOR2 | 128x128 | `b71bae2b662f1682be58e1517a0cc5f2b01aec3ebf9b3c9dee7d0ae7ed6d786e` | 7261  | 0.0078125,0.0078125             |
| wall | BIGDOOR4 | 128x128 | `a896c8326dcc51c30d2844e14acbfc24b5db317f4c6e860d208edfd3e2c58d75` | 6830  | 0.0078125,0.0078125             |
| wall | BRNBIGC  | 128x128 | `fc15b130a553aea061db2273ffb3192a5e898d7ea52b149000405bc0322fff80` | 5018  | 0.0078125,0.0078125             |
| wall | BRNBIGL  | 32x128  | `8505f1ea13c11bea0c12f82cb64f6baf3564d9de1529aa2108a6d69244f4851b` | 2217  | 0.03125,0.0078125               |
| wall | BRNBIGR  | 32x128  | `c4133c71c3ee61d5d28fa3c8df8aa57ee0e22b3cf920e80b7e314df868c8dd17` | 2381  | 0.03125,0.0078125               |
| wall | BROWN1   | 128x128 | `2834bb6c2e90168f9779df902d6eed2b1a3e1e3d98a082a9846a078beb22ac56` | 6510  | 0.0078125,0.0078125             |
| wall | BROWN144 | 128x128 | `fced9e2c70a8f86c00460b0ec1cdb80acc1ab942b695bb71cfcbec62509b7e63` | 5941  | 0.0078125,0.0078125             |
| wall | BROWN96  | 128x128 | `3c00b6ba87dd6d00fa9a7e3f1df23596c1856253a15f6d6b291519a1cd9065ac` | 6422  | 0.0078125,0.0078125             |
| wall | BROWNGRN | 64x128  | `3bce58e643d453b73480c7411c866079cf86b6b6c1c6aa0352a835b707a13321` | 3350  | 0.015625,0.0078125              |
| wall | COMPSPAN | 32x128  | `db1241741152f3347df68cd36991a320abc339f7025caadd917271ecd64cdb60` | 178   | 0.03125,0.0078125               |
| wall | COMPTALL | 256x128 | `2c4f6587ecd2542b14eafbd104b8c0f263523314cbc31d0b5b83918443c244f9` | 8965  | 0.00390625,0.0078125            |
| wall | COMPTILE | 128x128 | `59a95e2985c3c2ef7b7b4ee44c56c6dc76cfc22f07872b96ea3156e5af028348` | 3413  | 0.0078125,0.0078125             |
| wall | COMPUTE2 | 256x56  | `ed89cd8eabe4b475d8ab73735c30919fad4638f4b7d24149403849a10a4a2e74` | 4570  | 0.00390625,0.017857142857142856 |
| wall | DOOR3    | 64x72   | `da36dc35eb653b72f09ae25159c60384467121b41a78140a00b57e44904649e4` | 1731  | 0.015625,0.013888888888888888   |
| wall | DOORSTOP | 8x128   | `bd7b79b900e735f3d4b643bed2714d84234ad2b8891159c86a0f87918d5ef539` | 363   | 0.125,0.0078125                 |
| wall | DOORTRAK | 8x128   | `132e7c41fc8c2ea67868bcdff16eec298a16bbab9e1a67a28249c5a1839dca99` | 429   | 0.125,0.0078125                 |
| wall | EXITDOOR | 128x72  | `2501c028357eb476b292a5a7e410197efd0b682411a2bd70ecadf25a22d85662` | 7209  | 0.0078125,0.013888888888888888  |
| wall | EXITSIGN | 64x16   | `938c42fdbde4889cf3b96455a71ba66fa123414581f2f99ce54a635b0f6ae089` | 380   | 0.015625,0.0625                 |
| wall | LITE3    | 32x128  | `63d2f56caa650cf3c74920f6ab58a67fd2332e25d957b16266dd1a1a73e966a4` | 321   | 0.03125,0.0078125               |
| wall | NUKE24   | 64x24   | `e549beaf06cfa82ea6d4c9fa7f81af73fa0de69f6504ab822695a3cc1c283958` | 1103  | 0.015625,0.041666666666666664   |
| wall | PLANET1  | 256x128 | `c084de2994185b31090a926cfc280865db82f04015de74a79c5422098d454156` | 12260 | 0.00390625,0.0078125            |
| wall | SLADWALL | 64x128  | `3d6a991e7b0f20ab908a0362fda304e0ab100c8c9f126befc811d5b78029d484` | 1558  | 0.015625,0.0078125              |
| wall | STARG3   | 128x128 | `032535e3959581622076ab81fea34709eb0e06d6e2b7302a60497e25edb54e6b` | 6955  | 0.0078125,0.0078125             |
| wall | STARGR1  | 64x128  | `560ccedb9e872750b7bdfdb2d16f148306435fc2c7ccbd7f88c561302b597f0d` | 2878  | 0.015625,0.0078125              |
| wall | STARTAN1 | 64x128  | `7c7ce8aac3276d2dc00e415f808be9f831807bd9e6681664475117266b11bb3f` | 2443  | 0.015625,0.0078125              |
| wall | STARTAN3 | 128x128 | `b72c5adb0f268cdf61a5c43026f8cea186be4a46eb8d2de22a53c9c2942372be` | 7923  | 0.0078125,0.0078125             |
| wall | STEP1    | 32x8    | `dc082aae89b6ca6fdcc7f36ee83015a5fcdc6a78fcb9cf8249c136b679fa7b9d` | 226   | 0.03125,0.125                   |
| wall | STEP6    | 32x16   | `9de329bb95366757b2695f361122944ddb771b4e4d58ca0079a257a237e04e31` | 486   | 0.03125,0.0625                  |
| wall | SUPPORT2 | 64x128  | `d98692a6a82600dfe8a40ab6b6a3632a417b44b8e2b42c9e6a43af686bade978` | 1519  | 0.015625,0.0078125              |
| wall | SW1STRTN | 64x128  | `f73dc748f159a09d0845f90863ae5d1d6eb3c1fb0f8314868d98ab7198ee4b67` | 2990  | 0.015625,0.0078125              |
| wall | TEKWALL1 | 128x128 | `ddad6576ca9d517b76c0fe2a91d75d17b4c64faa9a4996ecfb3d4c8a3091c5ae` | 13248 | 0.0078125,0.0078125             |
| wall | TEKWALL4 | 128x128 | `fc3ce505cbdb132dae0b36f52c9cc7e46bd5c667b42822465c0c94b9df1efa5d` | 16327 | 0.0078125,0.0078125             |

Exact PNG bytes and hashes are closed by `content/doom-e1m1/textures/manifest.json`. Two golden flats (`FLOOR7_2`, `CEIL3_5`) are byte-equal to the reference `doom.ts` canvas rendering at the same `PLAYPAL` revision; wall provenance for `BIGDOOR2` includes `TEXTURE1` entry bytes (22 B) plus patch `W94_1` bytes.

### Derived voxel asset (single sparse-run volume, gameplay truth)

TS `voxelize(manifest, scale=16, offset=[−768,−136,−4864]) → VoxelAsset` produces `content/doom-e1m1/doom-e1m1.voxel.json` with
`voxelDataHash sha256:fad81c1c1d8b8ffe30b733817f70b494b26c1ca788e4c8a40a6fe16ffb6c756d`
`contentHash sha256:4119fe84f82e6fd98dc66e069eaede6b1faebcb32a86b738f116a97e3a78b65c`
`sparseRuns 14,476 / 49,908 resolved cells, bounds [0,0,0]-[286,24,176]`, `materialPalette` 54 entries mapping each flat/wall name to `material/doom-flat-*` / `material/doom-wall-*` (tileScale as above). Doom type-1 door spans remain represented by the authored Rust-owned door entities rather than duplicate immutable collision voxels, so opening those entities leaves the connected E1M1 route traversable. Budget `≤1M` voxels, `≤65k` resolved cells, verified by `cargo test -p loading-bay-game --test doom_voxel_asset` which decodes without mutation. Project `content/projects/doom-e1m1.project.json` file SHA-256 and current static revision are `sha256:29ef9b937ac0fbae1f68daa184cbd213be483a1a402c43edf407906a22620f7e`.

### Authored project

`content/projects/doom-e1m1.project.json` schema 24 `scene/doom-e1m1` embeds the voxel volume (`voxel-volume/doom-e1m1` at identity, plus `voxelEnvironment` material proxy referencing same asset) and 54 VTX6 materials (`material/doom-*` with `voxelSurface` repeat), 54 textures (`texture/doom-*`), and 41 mesh resources copied from `loading-bay` (`mesh/player-marker`, `mesh/prop-kit/*`, `mesh-animation/*`). One `StoredMaterialDefinition` per texture with `tileScaleCells`/`tileOriginCells` straight-alpha Nearest/Repeat. Project admits via `ProjectStore` canonical round-trip (4.6 MiB <8 MiB) and is listed in `libs/project-content` alongside `loading-bay`/`relay-annex`.

Task #6804 ports only E1M1's single-player weapon subset: the starting pistol,
the one placed shotgun, bullets, and shells. Thing types 2001, 2007, 2008,
2048, and 2049 and their single-player option bit are read from the hashed WAD;
multiplayer-only type-2002 and type-2003 placements are excluded. The reading
reference at `/home/research/doom.ts/src/doom/{game/game.ts,play/inter.ts,play/p-sprite.ts,play/local.ts,doom/items.ts,doom/info/states.ts}` establishes the
50-bullet start, 200/50 ammo bounds, 10/50 bullet grants, 4/20 shell grants,
eight shells with a found shotgun, one-ammo attacks, seven shotgun rays,
5/10/15 damage multiples, 2,048-Doom-unit range, held refire, and source state
cadence. The authored 60 Hz cooldowns are the nearest integral fixed-tick
calibration of those 35 Hz state intervals. Rust implements reusable held-fire,
cooldown, deterministic damage-roll, spread, hit, occlusion, ammo, vitality,
death, drop, fact, and snapshot owners; TypeScript only composes these values
and disposable presentation identities. Existing original Loading Bay prop
meshes remain temporary visual substitutes and are not represented as Doom
sprites or weapon art.

No `doom1.wad` is read at runtime; the browser receives only the immutable `RuntimeProjection` and typed facts. The offline forge is deterministic: `node dist/cli.js --check`, `node dist/texture-cli.js --check`, and `cargo run -p loading-bay-game --bin doom-voxel-hash -- doom-e1m1.voxel.json` are the re-producers.
