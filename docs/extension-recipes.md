# Extending Rusty Engine Demo

This is a game repository and reference consumer, not a plugin SDK. Extend the narrow owner that
already controls the behavior, keep the wire and presentation projections typed, and prove the
result in the real browser when it is visible. Do not add a registry, service locator, generic
method bridge, behavior graph, or second gameplay/render loop.

## Read the execution path first

```text
keyboard / pointer
  -> browser input capture and bounded LoadingBayGameSession command
  -> Rust LoadingBayGameLoop fixed tick
  -> named Rust service mutates a candidate GameSession atomically
  -> typed facts plus immutable runtime snapshot/projection
  -> bounded WebSocket full/delta update
  -> Angular view models and RuntimeProjectionAdapter
  -> one shared auto-started RendererSurface and disposable feedback hosts
```

Ownership and failure behavior are deliberate:

| Stage                                      | Owner                                                              | Failure rule                                                                                     |
| ------------------------------------------ | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| Authored definitions and level composition | `ts/packages/project-content` emits immutable JSON; Rust admits it | Invalid content fails before runtime publication                                                 |
| Device capture and host-user preferences   | browser TypeScript                                                 | Blur, pointer-lock loss, pause, disposal, or disconnect neutralizes bounded input                |
| Tick order and live gameplay state         | `LoadingBayGameLoop`, `GameRuntime`, and named Rust services       | Mutate a candidate session and publish only after every checked phase succeeds                   |
| Save/project persistence                   | Rust project store and save store                                  | Authored projects and runtime snapshots stay distinct; replacement is fail-atomic                |
| Transport                                  | game-specific Rust host plus `LoadingBayGameSession`               | No mutation retry; gaps resync, transport loss reconnects, queues reject visibly at their bounds |
| HUD and retained world projection          | Angular components and `RuntimeProjectionAdapter`                  | Immutable projection only; rejection never creates local inventory, combat, or progression state |
| Rendering and effects                      | exact-pinned shared Rusty Engine surface/hosts                     | Presentation may be dropped or reset without changing gameplay                                   |

Before changing code, read [the product architecture](fps-product-architecture.md), [the session
protocol](game-session-protocol.md), and the Den document
`rusty-engine-demo/known-limitations`. The latter is the canonical active limitation list.

## Recipe: add an item definition and pickup

Use this for another ammunition stack, access key, health supply, armor supply, or intentionally
inert item whose existing `ItemKind` already expresses its meaning.

1. Add the stable namespaced identity and immutable definition to
   `ts/packages/project-content/src/encounter-project.ts`.
2. Add a `pickupEntity(...)` at an authored translation. Pickups reference the item identity and
   quantity; do not add a browser click handler that grants it.
3. Regenerate `content/projects/*.project.json` with `pnpm run generate:content`.
4. Update the composer corpus in `encounter-project.test.ts` and the Rust pickup/inventory tests in
   `rust/crates/loading-bay-game/tests/` when the shipped baseline changes.
5. For visible content, update `docs/source-provenance.md` and make
   `pnpm run test:browser` walk through the accepted Rust pickup.

Rust admission is in `stored_project.rs` and `project_admission.rs`; live quantities and atomic
grant/consume behavior are in `item.rs`, `inventory.rs`, and `pickup.rs`. Add a new Rust item kind
only when the item has a genuinely new game-owned transaction. Do not encode behavior in an asset
name or a TypeScript switch.

The existing `key/inert-inspection-tag` is the locality proof: it required only the TypeScript
composer, checked-in generated JSON, composer assertion, and provenance. No Rust behavior changed.
Open an Engine task only if this work exposes a bounded, item-agnostic storage or trigger mechanism
already required by another real consumer. Item kinds, grant policy, capacity, pickup facts, and
use transactions remain Loading Bay Rust.

## Recipe: add a weapon and ammunition association

Use an existing explicit attack mode (`hitscan`, bounded `spread`, or `automatic`) when it fits.

1. Add the ammo item and weapon item definitions in `encounter-project.ts`. The weapon definition
   owns damage, range, cadence, ammo identity/cost, muzzle offset, and presentation identity.
2. Add the weapon identity to the player's authored `weaponSlots`; add a pickup and starter ammo
   only if the level calls for it.
3. Add an original renderer-neutral silhouette in
   `ts/packages/browser-shell/src/weapon-viewmodel.ts` and disposable cue mapping in
   `presentation-feedback.ts`.
4. Prove definition/ammo/cooldown/selection/snapshot behavior in
   `weapon_inventory_runtime.rs` and `game_loop.rs`; prove descriptor/reset behavior in
   `weapon-viewmodel.test.ts` and real selection/fire/dry-fire in Chromium.
5. Regenerate content and update provenance.

`combat.rs` and the fixed combat phase remain the only hit/ammo/damage authority. A new firing
mode is a Rust gameplay change: extend the closed stored schema, admission, definition, combat
service, facts, snapshot migration, TypeScript decoder/projection, and focused tests together.
Never implement a weapon as a TypeScript callback or a string-dispatched effect graph.

Loading Bay versus Relay Annex already proves content-only weapon tuning: the same arc-pistol
service admits different authored damage without a Rust branch.
Open an Engine task only for a renderer-neutral retained/picking/spatial capability with a second
consumer. Weapon modes, ammunition policy, cadence, hit selection, damage, and combat facts are
gameplay and stay here.

## Recipe: add an enemy content variant

Use this when sight/hearing range, body size, health, movement speed, attack kind/range/damage,
cadence, presentation identity, encounter membership, or deterministic drop is enough.

1. Compose another `enemyEntity(...)` in `encounter-project.ts` with a stable entity ID/name,
   explicit `enemyCombat`, navigation, collision/body bounds, vitality, render asset, and drop.
2. Put it in an explicit bounded encounter and define the dormant ordinary pickup that its defeat
   materializes.
3. Add a distinct primitive silhouette/material mapping in `projection.ts` when necessary.
4. Cover activation, tactical configuration, exact-once drop, reset, and snapshot reopen in
   `enemy_archetype_runtime.rs` and `enemy_combat_runtime.rs`.
5. Exercise the variant in `test:browser`; update `docs/source-provenance.md` for the original or
   permissively licensed source of every new visual/audio asset.

Bay Rusher, Arc Warden, and the Relay Annex health/navigation tuning use this path. They share the
same named Rust phases and services; there is no enemy subclass registry.
Open an Engine task only if the variant demonstrates a reusable bounded navigation, spatial, or
renderer-neutral mechanism in another consumer. Enemy identity, tuning, encounters, drops, and
presentation meanings remain downstream.

## Recipe: add genuinely distinct enemy behavior

First demonstrate why admitted configuration cannot represent the tactic. Then keep the addition
inside the existing explicit phase sequence.

1. Add a closed authored configuration/state shape in `stored_project.rs` and the game definition,
   with fail-closed legacy migration in `project_codec.rs`.
2. Admit all required composition up front in `project_admission.rs`. Missing vitality, collision,
   navigation, target, or spatial capability must reject before a tick can mutate.
3. Add or extend one named service in `enemy_combat.rs`/`runtime.rs`; call it from the documented
   `LoadingBayGameLoop` phase. Stage candidate state so a late quota/spatial error cannot partially
   mutate an earlier enemy.
4. Emit a closed typed fact and persist only durable state in `snapshot.rs`. Routes, LOS hits, and
   current query results remain derived/transient.
5. Extend the Rust integration corpus with cadence, ordering, occlusion, failure atomicity,
   death/restart, and snapshot eventual-outcome tests; then add typed browser projection/cues and
   a real product proof.
6. Update `docs/source-provenance.md` for every new original or permissive asset/cue source and
   record the schema/migration meaning added here.

Open a Rusty Engine task before coding only when the missing piece is renderer-neutral or
game-neutral mechanism already demanded by another real consumer—for example the reviewed
combined voxel/entity occlusion query. Pin and consume its exact public SHA. Enemy tactics,
phase order, damage policy, and facts stay here.

## Recipe: add a door, key, switch, or secret

1. Define the key as an item and place its pickup through the first recipe.
2. Author the door's collision bounds, closed/open transforms, optional schedule, and explicit
   access requirement/policy on that door entity.
3. Author direct switch-to-door identities. A Loading Bay-specific multi-door consequence belongs
   in the named switch service, not in a generic reaction graph.
4. Author secret bounds and first-discovery presentation on the responsible secret entity.
5. Test admission paths, missing/wrong key, retain/consume behavior, repeated interaction,
   scheduled consequences, first-discovery idempotence, save/reopen, death, and failure
   non-mutation in `progression_runtime.rs` and `door_runtime.rs`.
6. Project only prompts/posture/facts in the browser and exercise the complete interaction in
   Chromium.
7. Update `docs/source-provenance.md` for new door, key, control, secret, audio, or text assets.

The live owners are `door.rs`, `switch.rs`, and `progression.rs`. Browser route navigation or DOM
visibility must never open a door, grant a key, count a secret, or complete a level.
Open an Engine task only if a second consumer needs a smaller generic collision, trigger, or
retained-transform mechanism. Access policy, item consumption, switch consequences, discovery,
completion, facts, and persistence stay here.

## Recipe: author a level variation

1. Add a typed option or a second composition function in `encounter-project.ts`.
2. Change immutable room voxels/materials, responsible entity transforms/configuration, lights,
   pickups, encounter membership, or tuning. Use the same admitted schema and runtime services.
3. Keep material voxels in deterministic address order and entities in stable ID order.
4. Run `pnpm run generate:content`; require deep equality between the composer and checked-in JSON.
5. Add Rust admission/spatial checks if the variation changes collision/navigation assumptions,
   and run the real browser when the change is product visible.
6. Record original/permissive source and transformation provenance.

`relayAnnexStoredProject()` is the change-amplification example. It changes layout, start/encounter
placement, enemy health/navigation, weapon damage, and beacon radius through TypeScript content
only. The relevant implementation is localized to the composer, its generated artifact, composer
tests, and provenance; it does not add a Rust loop, protocol, or renderer branch.
Open an Engine task only when the variation cannot be admitted by an existing game schema because
of a reusable authoring, voxel, spatial, or renderer-neutral bound already required elsewhere.
Room route, encounter composition, tuning, object names, and progression remain downstream content.

## Locality evidence

These examples report their actual change surface; none hides a runtime feature behind “content
only”:

| Variation                                               | Files changed                                                                                                  | Proof                                                                                    | Owners intentionally unchanged                       |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| Add inert inspection-tag definition                     | `encounter-project.ts`, both generated project JSON files, `encounter-project.test.ts`, `source-provenance.md` | definition present, absent from starting stacks, generated deep equality                 | Rust services/schema, protocol, renderer             |
| Tune arc-pistol and enemy configuration for Relay Annex | `encounter-project.ts`, `relay-annex.project.json`, `encounter-project.test.ts`                                | weapon definitions differ; enemy vitality/navigation differ                              | combat/enemy services and phase order                |
| Rearrange Relay Annex layout and player start           | `encounter-project.ts`, `relay-annex.project.json`, `encounter-project.test.ts`                                | voxel environments and player translations differ; generated artifact equals composition | Rust admission/runtime, WebSocket, Angular, renderer |

The test `settled items, weapon tuning, enemy tuning, and layout remain content-local` is the
single executable summary. If a proposed “variation” also needs schema, migration, facts, wire
decoding, or a named service, report those files honestly and follow the corresponding behavior
recipe instead.

## Forbidden shortcuts and enforced checks

| Shortcut                                                                               | Why it is wrong                                         | Detection                                                                                       |
| -------------------------------------------------------------------------------------- | ------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| TypeScript inventory/combat/damage/pickup/enemy services or legacy HTTP mutators       | Creates a second gameplay authority                     | `pnpm run audit:boundary`, Rust service/failure tests, browser accepted-state proof             |
| Fake transport, placeholder actions, inert enabled controls                            | Makes the product claim behavior it does not have       | boundary marker audit plus real Chromium menu/action proof                                      |
| Copied renderer packages, Three internals, sibling paths, or floating Engine revisions | Forks reusable authority and breaks clean consumers     | exact dependency/path/package checks in `audit:boundary`                                        |
| `requestAnimationFrame` or a second `renderOnce` in downstream code                    | Creates another renderer/effect scheduler               | `audit:boundary`; shared surface lifecycle browser proof                                        |
| Voxel meshes/lights in ordinary dynamic deltas                                         | Repeats cold resources and defeats bounded transport    | `StaticStateKey`/`RuntimeDynamicState` audit plus session tests and live static-update counters |
| Local effect/resource caches or gameplay callbacks from audio/particles/viewmodels     | Lets presentation survive/reset or mutate independently | shared host usage, projection commit tests, reload/disposal Chromium proof                      |
| Hand-editing generated project JSON                                                    | Splits authored truth                                   | `pnpm run check:content`                                                                        |

The audit catches known structural regressions; it is not permission to bypass code review with a
different spelling. When a change crosses an owner, state the new contract and failure behavior in
the task handoff.

## Verification and handoff

Use the smallest focused loop while editing, then the complete gate for any visible or
cross-language change:

```bash
pnpm run generate:content
pnpm run check:content
pnpm run test:ts
./scripts/verify-rust.sh
pnpm run test:shell
pnpm run test:boundaries
pnpm run lint
pnpm run typecheck
pnpm run build:shell
pnpm run test:browser
pnpm run audit:boundary
pnpm run verify
```

In Den, record the exact public head SHA and useful diff base, commands actually run, live browser
evidence, artifact/provenance changes, any accepted limitation, and the expected GitHub check name
(`verify`). A pushed commit is not approval; request an independent task review and address every
finding through a fresh rereview.
