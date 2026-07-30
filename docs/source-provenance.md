# Source provenance

Rusty Engine Demo was originally extracted from
[`FuzzySlipper/rusty-engine`](https://github.com/FuzzySlipper/rusty-engine) at commit
`a2e55f9660e46751d4c78bcdd23b9a321b0dc961` under Den task #6137.

The current Rust dependencies, browser render packages, and Studio composition packages resolve
from the one exact public Engine revision in [`engine-source.json`](../engine-source.json). The
older revisions below remain historical extraction and feature-provider points, not active
dependency pins.

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

Reusable Rust crates are not copied. Cargo consumes their packages directly from the exact Engine
Git revision recorded in `Cargo.toml` and `Cargo.lock`.

The original Engine repository contains the historical Asha donor provenance for low-level code.
This repository records its immediate Engine source and does not recreate the old donor hierarchy or
runtime claims.

## M10B browser transfer

| Local surface                                 | Source path | Treatment                                                                                                                                                             |
| --------------------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ts/packages/project-content`                 | same path   | Copied as the immutable fixture composer and schema/assertion package, then narrowed under #6353 so it cannot reproduce or overwrite canonical Studio project scenes. |
| `ts/packages/browser-shell`                   | same path   | Copied as the product-owned input, projection, feedback, and browser shell; imports renamed to the demo package scope.                                                |
| `ts/packages/render-contracts`                | same path   | Initially copied into the demo; removed under #6162 after its complete successor became a shared exact-revision Engine package.                                       |
| `ts/packages/renderer-three`                  | same path   | Initially copied into the demo; removed under #6162 after the retained Three/WebGL backend and browser surface moved behind shared Engine packages.                   |
| `scripts/browser-smoke.mjs`                   | same path   | Copied as the end-to-end product proof; the Rust package invocation changed from the source product name to `loading-bay-game`.                                       |
| Root pnpm, TypeScript, and Vite configuration | same paths  | Copied and narrowed to the demo-owned package identities and verification gate.                                                                                       |

The browser packages initially moved together because all four served one product at extraction
time. The later renderer migration made the demo an external consumer of
`@rusty-engine/render-contracts`, `render-projection`, `renderer-host`, and `renderer-three`.
Only the game-specific input, semantic projection, and typed fact-to-descriptor mapping remain here.

Den task #6378 advances that complete package family to Engine
`a6857d03141e162511231c276ee751a3413c90e5` and consumes only the public immutable
`RendererSurface.submission()` sample for renderer statistics. The downstream proof adds no Three
import, WebGL inspection, private renderer object, resource cache, or second render loop.

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

The authored primitive asset identities in the campaign resolve through the existing downstream
projection adapter and the exact-pinned shared renderer. They are not imported art files. The only
checked-in third-party visual assets remain the separately itemized Kenney CC0 sources above.
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
reviewed Engine descendant `a6857d03141e162511231c276ee751a3413c90e5`.
The downstream call site reads `surface.timing()` from the shared auto-started surface; no demo
frame scheduler, backend clock, renderer object access, or private renderer was introduced.
