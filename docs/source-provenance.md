# Source provenance

Rusty Engine Demo was originally extracted from
[`FuzzySlipper/rusty-engine`](https://github.com/FuzzySlipper/rusty-engine) at commit
`a2e55f9660e46751d4c78bcdd23b9a321b0dc961` under Den task #6137.

The current Rust dependencies resolve from exact reviewed authoring revision
`ad19a0a6e74af711875a9ce0d113b9f231e434ec`; browser render packages resolve from exact review-fix
revision `2665b74566136fb77e3a26b0766394124c8f58d3`. The older revision below remains the historical
extraction point, not an active dependency pin.

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

| Local surface                                 | Source path | Treatment                                                                                                                                           |
| --------------------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ts/packages/project-content`                 | same path   | Copied as the optional immutable content composer and renamed to `@rusty-engine-demo/project-content`.                                              |
| `ts/packages/browser-shell`                   | same path   | Copied as the product-owned input, projection, feedback, and browser shell; imports renamed to the demo package scope.                              |
| `ts/packages/render-contracts`                | same path   | Initially copied into the demo; removed under #6162 after its complete successor became a shared exact-revision Engine package.                     |
| `ts/packages/renderer-three`                  | same path   | Initially copied into the demo; removed under #6162 after the retained Three/WebGL backend and browser surface moved behind shared Engine packages. |
| `scripts/browser-smoke.mjs`                   | same path   | Copied as the end-to-end product proof; the Rust package invocation changed from the source product name to `loading-bay-game`.                     |
| Root pnpm, TypeScript, and Vite configuration | same paths  | Copied and narrowed to the demo-owned package identities and verification gate.                                                                     |

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

## M11B Studio adapter and voxel-owner adaptation

The project-owned Studio adapter was authored directly in this repository against an exact Engine
revision, now `ad19a0a6e74af711875a9ce0d113b9f231e434ec`. It composes the public `asset-catalog`,
`authored-scene`, `content-store`, `entity-state`, `engine-inspector`, `render-model`, and
`render-projection` crates while retaining Loading Bay schema, layout, and domain admission here.
No Studio or adapter implementation was copied from Engine or Asha.

That Engine revision also made the converted-voxel owner contract explicit. The checked-in
`kenney-wall-a.voxel.json` and its embedded project copy retain the same source mesh, sparse voxel
runs, material mapping, bounds, and provenance, but are re-encoded with the Engine-owned material
palette plus voxel-data and content hashes. This is a successor-codec adaptation of the existing
artifact, not new product content or a restored Asha dependency.

## M11F Studio parity adaptation

Protocol 6 was authored directly in this repository against Engine revision
`ad19a0a6e74af711875a9ce0d113b9f231e434ec`. It adapts the Engine-owned source-asset import,
catalog/lock, renderer payload, voxel primitive/template, history, annotation, conversion, canonical
asset, and environment-authoring mechanisms into named Loading Bay project operations. Trusted
host-file publication, source-drift inspection, and private prepared-candidate state are downstream
adapter responsibilities. No Asha replay, facade, generic command, Studio, or demo topology was
transferred.

The animated appearance proof uses Kenney's CC0 `Animated Characters Retro` medium character as a
checked-in product asset rather than an Engine fixture:

- `content/assets/kenney-retro-character-medium.glb`: 217,536 bytes, SHA-256
  `c71255a41c0373f0d2ef52593369d5fd9d2f6220ae548aff8cd6bf5edb403674`
- `content/assets/KENNEY-ANIMATED-CHARACTERS-RETRO-LICENSE.txt`: 665 bytes, SHA-256
  `d344fd83cc72bedadecbf2d051b904d3e63378cb87b489122a3efdb850b7ca7c`

The project catalog admits that exact content hash and its named `idle`, `run`, and `jump` clips;
Studio resolves the bytes through its bounded trusted-host resource path and the shared renderer.

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

The public `/home/dev/rusty-engine-ui` checkout at exact commit
`68ddfa5430ec3bc2cf7ca96963982db9511e79ba` supplied the following #6216 downstream shell patterns:

| Local surface                                                                                     | Donor surface                                                                                                       | Treatment                                                                                                                                                                                              |
| ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Root Angular 21 / Nx 23 workspace, boundary tags, strict app TypeScript, and pnpm build allowlist | root `package.json`, `pnpm-workspace.yaml`, `nx.json`, `boundaries.json`, `eslint.config.mjs`, `tsconfig.base.json` | Selectively adapted around this repository's existing Rust and `ts/packages/project-content` owners; package names, scopes, targets, and paths are Loading Bay-specific.                               |
| `libs/theme`                                                                                      | `libs/theme`                                                                                                        | Palette and reusable panel-token structure adapted to the existing Loading Bay visual language.                                                                                                        |
| `libs/platform`                                                                                   | `libs/platform`                                                                                                     | Narrow `DocumentEffectsPort` and browser implementation selected; unused storage, clock, and clipboard ports were not copied.                                                                          |
| `libs/ui-compass`                                                                                 | `libs/ui-compass`                                                                                                   | Pure presentation algorithm and component structure adapted to the full-viewport FPS overlay.                                                                                                          |
| `libs/ui-combat-log`                                                                              | `libs/ui-combat-log`                                                                                                | Pure presentation structure adapted to committed Rust fact projections.                                                                                                                                |
| `apps/loading-bay`                                                                                | `apps/app` foundation                                                                                               | Angular bootstrap, hash-router ownership, and standalone-component patterns adopted; the actual viewport, HUD, diagnostics, input, renderer, and runtime projection remain this game's implementation. |

No donor feature screen, fake transport, demo configuration, placeholder action provider, store
kernel, UI-owned inventory/equipment state, or inert menu control was imported. The migrated route
mounts the exact shared Engine renderer already used here and disposes its input listeners, held
input, presentation feedback, and shared surface through one route-owned lifecycle.

Rusty Engine task #6213 produced the renderer-owned timing seam at public SHA
`2665b74566136fb77e3a26b0766394124c8f58d3`. That SHA is recorded here as reviewed-upstream
integration evidence and is the active exact browser-renderer dependency pin adopted by #6219.
The downstream call site reads `surface.timing()` from the shared auto-started surface; no demo
frame scheduler, backend clock, or private renderer access was introduced.
