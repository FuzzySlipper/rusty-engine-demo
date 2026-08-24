# Downstream onboarding and clean clone

Loading Bay is the **advanced complete product reference** for Rusty Engine. The
minimal starter is the Engine repository's `fixtures/rust-sdk-consumer` crate;
this repository is what a real product looks like after content, mechanics,
transports, and tooling arrive. Do not copy Loading Bay wholesale to start a new
product — copy the spine below, then take only the named pieces you need.

## Clean-clone prerequisites

- A sibling checkout of Rusty Engine at `../rusty-engine` (the Cargo path
  dependency in the root `Cargo.toml` and the `file:` TypeScript dependencies
  resolve against it; there is no registry publish).
- Node ≥ 26 with pnpm ≥ 11 (`corepack enable`), Rust stable (1.96+ tested).
- Linux native dependencies for winit/wgpu development builds (the usual
  `libxkbcommon`, `libudev`, Vulkan loader set) and Tauri system deps only if
  you build the desktop bundle.
- First build order:
  1. `pnpm install --frozen-lockfile`
  2. `pnpm run build:shell` — builds the Angular shell the hosts serve.
  3. `cargo run --locked -p loading-bay-game --bin browser-host --release`

No WAD or other external asset is required to build, verify, or run: the
canonical project and its derived assets are committed. Regenerating derived
content from sources additionally requires the id Software shareware IWAD at
the path recorded in [source-provenance.md](source-provenance.md).

## Generation and admission order

1. `pnpm run gameplay:build` — compiles `gameplay/authoring/` TypeScript into
   the canonical binary64 gameplay package under `data/gameplay/`.
2. `node ts/packages/doom-e1m1-authoring/dist/compose-project.js --write` (or
   `pnpm --filter @rusty-engine-demo/doom-e1m1-authoring generate`, which also
   re-derives textures/sprites and **requires the WAD**) — composes
   `content/projects/doom-e1m1.project.json` from the committed intermediate
   and manifests.
3. Admission is load-time: `LoadingBayProductService::admit` decodes the
   canonical project through the current stored-project schema and Engine
   codecs. There is no migration path from old schemas; re-author instead.

The default gates check every layer without a WAD:

```bash
pnpm run gameplay:check   # authored rules package drift
pnpm run check:content    # canonical project vs authoring sources + schema + canonical bytes
pnpm run verify           # typecheck + all of the above + tests + shell + Rust authority
```

`pnpm run check:provenance` re-derives intermediate/sprites/textures from the
WAD itself and requires the provenance path above.

## Minimum copyable spine

A new downstream needs exactly this much to be "an Engine game":

1. One Cargo dependency on the complete adjacent facade
   (`rusty-engine = { path = "../../rusty-engine" }`). Never select subcrates.
2. One Rust product service that admits your authored project and owns the
   fixed-step loop, semantic commands, saves, and readouts (here:
   `LoadingBayProductService`).
3. Immutable TypeScript authoring that composes content and emits one
   canonical artifact admitted by that service (here: `gameplay/authoring/`
   plus `ts/packages/doom-e1m1-authoring`).
4. A thin composition root serving that shell over HTTP/WebSocket (here:
   `browser-host`), using the public `@rusty-engine/application-host` package.
5. Optionally, an in-process Tauri adapter over the same service (here:
   `src-tauri`) with no sidecar process.

Everything else in this repository is product complexity with a named owner,
catalogued below.

## Ownership and code-routing table

| Concern | Reusable Engine pattern | Loading Bay product policy (do not copy blindly) |
| --- | --- | --- |
| Durable entity facts | Engine typed component store: `ComponentRegistry`, durable codecs, revisions, snapshot round-trip | The 19 registered `loading-bay.*` fact types and their Doom meaning (`gameplay/src/facts.rs`) |
| Generic scene structure | Engine `authored-scene` document: labels, hierarchy, transforms, renderable assets, lights; admission owns all generic invariants | Binding records only: gameplay components keyed by node id plus visibility/clip/binding presentation overrides (`StoredEntityDefinition`) |
| Validation | Engine admission diagnostics translated onto product paths (`definition_error`, scene-validator probe) | Doom cross-component relationships, program bindings, weapon caps, trigger requirements |
| Inventory mutations | `StandardOperation::{GrantStack, ConsumeStack, EquipUniqueItem, UnequipUniqueItem, SwapUniqueItem}`, `ItemService::materialize_unique` | Distinct-stack capacity, weapon slots/reserved identities, None-endpoint disposal/reacquisition, command sequences (`InventoryService`) |
| Vitality | Engine tracks/damage/effects, ActionActor/DestructibleResource presets, `ReplaceEffect` | Armor split/order, depletion consequences, supply interpretation, HUD receipts (`DamageService`) |
| Provenance | Typed Engine receipts returned by every standard leaf/service call | Product facts/views are orchestration; always retain the Engine receipts on the result (`InventoryReceipt.standard_receipts`, `VitalityReceipt.standard_receipts`) |
| Spatial authority | One admitted `VoxelCollisionScene`; `SpatialOcclusionService::cast_ray`; `VoxelRenderProjector` | Trigger reconciliation, navigation, combat targeting policy (hitbox extents, eligibility) |
| Rendering | Public `@rusty-engine/application-host`; renderer/canvas owned by Engine | Semantic input capture, disposable HUD (`apps/loading-bay`, `ts/packages/browser-shell`) |
| Transports | Host-neutral service surface | `browser-host` WebSocket adapter, typed Tauri IPC — adapters, never gameplay owners |

Compatibility machinery that exists only for history: legacy-snapshot migration
paths (`snapshot.rs` legacy carriers, `attach_legacy_weapon_item`) and the
pre-reservation save shapes. New products should not copy these.

## Naming map

A few historical names do not describe their behavior:

| Name | Actual behavior |
| --- | --- |
| `pnpm run generate:content` | Regenerates **all** derived content (textures/sprites need the WAD); then rebuilds the gameplay rules package. |
| `pnpm run check:content` | Default-gate drift check: recomposes the canonical project from committed sources and compares bytes, plus schema validation. No WAD needed. |
| `pnpm run check:provenance` | Full WAD re-derivation of intermediate/sprites/textures (requires the provenance path). |
| `@rusty-engine-demo/project-content` | Schema types + canonical-file validation — it does not generate content; the generator lives in `@rusty-engine-demo/doom-e1m1-authoring`. |
| Nx project `loading-bay` | The Angular shell app under `apps/loading-bay` (not the Rust product). |

## Developer console

The public developer-command surface (discovery, safe-point bindings, standard
inspect/admin commands, product schemas, cancellation/disposal, browser
WebSocket + Tauri IPC adapters, and the rule that UI expresses intent while
Rust owns mutation) is specified in [design.md](design.md) ("The
developer-command client...") with an implementation walk in
[extension-recipes.md](extension-recipes.md). The generated client and schemas
live in `ts/packages/browser-shell/src/developer-command.ts`; Engine owns the
contract (`../rusty-engine/docs/inspection-and-diagnostics.md`).

## Presentation frame

Rendering, loading, failure, indicators, and interactive UI stay inside one
bounded viewport; gutters reject input. See
[presentation-frame.md](presentation-frame.md) and why that makes deterministic
browser proof transferable to Tauri.

## Where everything lives

- `docs/design.md` — authority boundary, closed program families, console contract.
- `docs/source-provenance.md` — E1M1/WAD/asset closure and hashes.
- `docs/presentation-frame.md` — bounded-viewport presentation contract.
- `docs/game-session-protocol.md` / `docs/tauri-desktop.md` — transport contracts.
- `docs/extension-recipes.md` — safe extension walks.
- Engine-side reading: `../rusty-engine/docs/rust-sdk-capabilities.md`,
  `../rusty-engine/docs/code-map/`, and the minimal fixture at
  `../rusty-engine/fixtures/rust-sdk-consumer`.
