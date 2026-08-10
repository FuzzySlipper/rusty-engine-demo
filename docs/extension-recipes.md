# Extending Rusty Engine Demo

This is a game repository and reference consumer, not a plugin SDK. Extend the narrow owner that
already controls the behavior, keep the wire and presentation projections typed, and prove the
result in the real browser when it is visible. Do not add a registry, service locator, generic
method bridge, behavior graph, or second gameplay/render loop.

## Read the execution path first

```text
keyboard / pointer
  -> native Engine adapter readout or browser semantic input capture
  -> Rust LoadingBayGameLoop fixed tick
  -> named Rust service mutates a candidate GameSession atomically
  -> typed facts plus immutable runtime snapshot/projection
  -> Engine-owned Rust frame/input/pick operations for the rendered native product
     or bounded WebSocket full/delta update for the browser HUD/control shell
```

Ownership and failure behavior are deliberate:

| Stage                                      | Owner                                                                            | Failure rule                                                                                     |
| ------------------------------------------ | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Authored definitions and level composition | Canonical `content/projects/*.project.json` saved through Studio; Rust admits it | Invalid content fails before runtime publication                                                 |
| Device capture and host-user preferences   | browser TypeScript                                                               | Blur, pointer-lock loss, pause, disposal, or disconnect neutralizes bounded input                |
| Tick order and live gameplay state         | `LoadingBayGameLoop`, `GameRuntime`, and named Rust services                     | Mutate a candidate session and publish only after every checked phase succeeds                   |
| Save/project persistence                   | Rust project store and save store                                                | Authored projects and runtime snapshots stay distinct; replacement is fail-atomic                |
| Transport                                  | game-specific Rust host plus `LoadingBayGameSession`                             | No mutation retry; gaps resync, transport loss reconnects, queues reject visibly at their bounds |
| HUD and browser readout                    | Angular components and browser-shell view models                                 | Immutable readout only; rejection never creates local inventory, combat, or progression state    |
| Rendering and effects                      | Engine-owned Rust host adapter and private renderer artifact                     | Presentation may be dropped or reset without changing gameplay                                   |

Before changing code, read [the product architecture](fps-product-architecture.md), [the session
protocol](game-session-protocol.md), and the Den document
`rusty-engine-demo/known-limitations`. The latter is the canonical active limitation list.

## Recipe: add an item definition and pickup

Use this for another ammunition stack, access key, health supply, armor supply, or intentionally
inert item whose existing `ItemKind` already expresses its meaning.

1. Open the canonical project in Studio and add the stable namespaced item definition.
2. Add a pickup entity at an authored translation. Pickups reference the item identity and
   quantity; do not add a browser click handler that grants it.
3. Save through the adapter, reload, and run `pnpm run check:content` to require stable JSON plus
   Rust admission and exact round-trip.
4. Update the semantic artifact assertions in `encounter-project.test.ts` and the Rust
   pickup/inventory tests in `rust/crates/loading-bay-game/tests/` when the shipped baseline
   changes.
5. For visible content, update `docs/source-provenance.md` and make
   `pnpm run test:browser` walk through the accepted Rust pickup.

Rust admission is in `stored_project.rs` and `project_admission.rs`; Loading Bay item identities,
kinds, definitions, slot/cooldown policy, and command translation are in `inventory.rs`.
`mechanics.rs` admits those definitions through the rolling Engine facade's gameplay-mechanics catalog,
whose components and named services own live quantities, containment, equipment, tracks, effects,
damage, and healing. Pickup transactions remain in `pickup.rs`. Add a new Rust item kind only when
the item has a genuinely new game-owned transaction. Do not encode behavior in an asset name or a
TypeScript switch.

The existing `key/inert-inspection-tag` is the locality proof: it required only a canonical project
edit, semantic artifact assertion, and provenance. No Rust behavior changed.
Open an Engine task only if this work exposes a bounded, item-agnostic storage or trigger mechanism
already required by another real consumer. Item kinds, grant policy, capacity, pickup facts, and
use transactions remain Loading Bay Rust.

## Recipe: add a weapon and ammunition association

Use an existing explicit attack mode (`hitscan`, bounded `spread`, or `automatic`) when it fits.

1. Add the ammo and weapon item definitions through the canonical Studio project. The weapon
   definition owns damage, range, cadence, ammo identity/cost, muzzle offset, and presentation
   identity.
2. Add the weapon identity to the player's authored `weaponSlots`; add a pickup and starter ammo
   only if the level calls for it.
3. Add the weapon's renderer-neutral retained description in the downstream Rust presentation
   owner used by `native-host`; keep semantic weapon identity and action consequences in gameplay
   Rust.
4. Prove definition/ammo/cooldown/selection/snapshot behavior in
   `weapon_inventory_runtime.rs` and `game_loop.rs`; prove frame/resource/reset behavior through
   `pnpm run verify:native` and browser HUD selection/fire/dry-fire through the session shell.
5. Save/reload the canonical project, run `pnpm run check:content`, and update provenance.

`combat.rs` and the fixed combat phase remain the only hit/ammo/damage authority. A new firing
mode is a Rust gameplay change: extend the closed stored schema, admission, definition, combat
service, facts, snapshot migration, TypeScript decoder/projection, and focused tests together.
Never implement a weapon as a TypeScript callback or a string-dispatched effect graph.

Loading Bay versus Relay Annex already proves content-only weapon tuning: the same arc-pistol
service admits different authored damage without a Rust branch.
Open an Engine task only for a renderer-neutral retained/picking/spatial capability with a second
consumer. Weapon modes, ammunition policy, cadence, hit selection, damage, and combat facts are
gameplay and stay here.

## Recipe: add an Engine rigid-body projectile weapon

Use this when a weapon's projectile must be a real Engine rigid body rather than a hitscan ray or a
browser-side visual effect.

1. Confirm the adjacent Engine facade exposes `engine-spatial::RigidBodyService`. Do not add a
   selective Engine dependency, revision machinery, or a demo physics loop.
2. Extend the downstream stored weapon schema with one closed `projectile` configuration: bounded
   mass, sphere radius, impulse, gravity scale, lifetime, restitution, and the existing ammo/damage
   policy. Validate the complete candidate before publication and reject projectile fields on other
   attack modes.
3. Spawn the projectile in the Rust combat service by consuming ammo and admitting one runtime
   entity with a bounded primitive collider, then apply the initial impulse through
   `RigidBodyAction`. The fixed game loop owns stepping; the browser receives only the immutable
   projection and typed facts.
4. Treat Engine contacts and authoritative target overlap as inputs to one downstream impact
   transaction. Apply damage at most once, destroy the projectile on impact or bounded expiry, and
   preserve the candidate-session/commit boundary if any validation or damage operation rejects.
5. Save only the durable weapon definition, inventory, cooldown, and gameplay state. Strip active
   projectile entities and their tombstones from runtime snapshots; a save/reopen must not resurrect
   a solver body or pending impulse.
6. Prove the real path with the canonical project: select the authored weapon, fire through the game
   runtime, observe an Engine-rigid-body motion receipt, verify impact/expiry and one-shot damage,
   save/reopen, and inspect the browser projection. Focused Rust proof is required; product-visible
   changes also run `pnpm run verify`.

The Loading Bay `weapon/kinetic-launcher` and `ProjectileService` are the reference consumer. The
Engine owns body integration, contacts, and rigid-body state publication; Loading Bay owns weapon
meaning, ammo, cooldown, target selection, damage, expiry policy, and combat facts. If the Engine
surface cannot express a required generic invariant, open an upstream task and keep this consumer
blocked rather than adding a local substitute.

## Recipe: expose a game-owned project component in Studio

Follow the Loading Bay Weapon path when a durable downstream component needs a typed Studio form:

1. Freeze a small named downstream contract and fixtures under `contracts/`. Do not add the
   component value, fields, or operation name to Engine's core Studio protocol.
2. Put the read, candidate validation, complete-project admission, optimistic guards, and atomic
   publication in the responsible downstream Rust owner. The concrete example is
   `rust/crates/loading-bay-game/src/weapon_authoring.rs`, routed as a closed companion operation by
   `rust/crates/loading-bay-game/src/studio_adapter/service.rs`.
3. Emit only the bounded owner/component/contract identity through the core adapter readout. A
   contract-bearing read is invalid until `describe` has advertised that exact contract version.
4. Put the strict decoder, concrete transport, disposable form lifecycle, and panel in one
   downstream feature package. The example is `libs/studio-weapon-inspector`.
5. Import the Engine built-ins and the downstream contribution explicitly in the product
   application root. See `apps/loading-bay-studio`; do not use an Angular multi-provider, registry,
   adapter-selected module, or runtime package scan.
6. Acquire the host mutation lease before the named downstream replacement and settle it with the
   Rust receipt's before/after project hashes. The host owns serialization and canonical reread;
   the panel owns neither the workspace store nor project authority.
7. Test exact version matching, unsupported identity-only fallback, read, replacement, semantic
   rejection, host-busy rejection, stale project or selection disposal, canonical reread, and
   fresh-process persistence.

Changing the downstream contract shape requires a new positive contract version and an exact
matching panel. Do not make version 1 permissive and do not teach Engine a generic component schema
to avoid that version cut.

## Recipe: add a serialized visual asset

Use the real Studio/adapter asset path for a new actor, prop, landmark, or reusable voxel brush:

1. Put the source and its license notice under `content/assets`, recording the original path or URL,
   author, license, byte length, SHA-256, and every Blender/conversion modification in
   `docs/source-provenance.md`.
2. Open `content/projects/loading-bay.project.json` in Studio. Import or author the asset through
   the named adapter operation, then inspect its catalog identity, source hash, dependencies,
   bounds, material slots, clips or voxel-object content hash, and aggregate project budget.
3. Attach the resource to a stable entity or place a voxel-object instance through the checked
   Studio viewport workflow. A voxel-object entity may select greedy cubes, marching cubes, or
   dual contouring in its built-in inspector; that selection is a guarded Rust project mutation,
   not local renderer state. Keep collision, navigation, interaction, pickup, and trigger proxies
   explicit on responsible entities; visual bounds do not become gameplay truth.
4. Save through the adapter mutation lease, reload, start a fresh adapter process, and verify the
   same project/content hashes. `pnpm run generate:content` must leave the canonical project bytes
   untouched.
5. Run `pnpm run check:content`, focused Studio/renderer lifecycle tests, and the real browser for
   visible content. Missing resources, stale hashes, one-over bounds, reset, disposal, and picking
   must fail or recover through their typed owners.

Do not hand-write renderer payloads, import Three loaders, copy an Engine package, or retain a
second TypeScript catalog. Reimport is a guarded canonical project replacement, not an alternate
asset cache.

## Cold-agent visual-content reproduction

These two recipes reproduce the campaign's production authoring paths without touching the
canonical project. They deliberately copy the complete `content` tree because import sources and
license notices are project-scoped. Inspect the resulting JSON diff and evidence before choosing
whether to repeat a mutation against the canonical artifact.

### Import and reimport one animated actor

The checked Blender recipe reads the Kenney Animated Characters Retro source pack and emits both
reviewed variants. To reproduce the source binaries from the locally installed pack:

```bash
PYTHONPATH=/usr/lib/python3.14/site-packages blender \
  --background --factory-startup \
  --python scripts/blender/build-loading-bay-actor-library.py -- \
  --source-root /home/stash/mesh-resources/kenney_animated-characters-retro \
  --output-dir content/assets/actor-kit
node scripts/check-actor-kit.mjs
```

The check requires Blender 5.1.2 output recorded by
`content/assets/actor-kit/source-manifest.json`: two GLBs, one embedded skin each, and the exact
`idle`, `run`, `jump`, `attack`, `hit`, and `death` clip set. Re-run the guarded import/reimport
against an isolated project:

```bash
actor_worktree="$(mktemp -d)"
cp -a content "${actor_worktree}/content"
git show f3eec1114a3835af2b694867f8677c9f4accfb7e:content/projects/loading-bay.project.json \
  >"${actor_worktree}/content/projects/loading-bay.project.json"
node scripts/author-actor-kit.mjs \
  "${actor_worktree}/content/projects/loading-bay.project.json" \
  "${actor_worktree}/actor-kit-authoring.json" \
  "${actor_worktree}"
node scripts/check-actor-kit.mjs
```

The exact reviewed `f3eec111…` project predates the actor import while the copied current `content`
tree supplies the reviewed project-scoped GLBs and notice. The receipt must report the same source
hashes, a typed stale-hash non-mutation, structural import of both assets, a no-op reimport for
unchanged source, byte-stable save/reopen, and the same hash from a fresh adapter process. Delete
the temporary directory after inspecting it. For the public viewport proof, start the supported
Studio host for that copy, set `RUSTY_STUDIO_ACTOR_HOST`, and run
`node scripts/capture-actor-kit-studio.mjs`; the committed reference evidence places all six clips
on both identities and proves +2 geometry, +2 material, +2 texture, and +12 independently animated
instances through resize, close/open, reload, remount, and disposal.

### Author, place, duplicate, save, and reload one voxel brush

The brush source is original Loading Bay geometry. Blender emits bounded GLB and mesh-JSON inputs;
Studio converts those sources into canonical voxel-object definitions and owns placement:

```bash
blender --background --factory-startup \
  --python scripts/blender/build-loading-bay-brush-kit.py
node scripts/check-brush-kit.mjs

brush_worktree="$(mktemp -d)"
cp -a content "${brush_worktree}/content"
pnpm run prepare:brush-reproduction -- \
  "${brush_worktree}/content/projects/loading-bay.project.json"
node scripts/author-brush-kit.mjs \
  "${brush_worktree}/content/projects/loading-bay.project.json" \
  "${brush_worktree}/voxel-brush-kit-authoring.json"
```

The preparation command refuses the canonical project and is intended only for a disposable copy.
It removes the current 367 presentation-only brush owners and their nine
material/mesh/object-definition triples, rejecting any owner with a gameplay component; every
gameplay entity, proxy, and binding remains. The authoring receipt must then contain nine
definitions, 25 request-ordered proof-room instances, exact object content hashes, aggregate
admission, canonical reread, and fresh-adapter persistence. Start the supported Studio host on that
isolated project, set `RUSTY_STUDIO_BRUSH_HOST`, and run:

```bash
RUSTY_STUDIO_AUTHORING_EVIDENCE="${brush_worktree}/voxel-brush-kit-authoring.json" \
  node scripts/capture-brush-kit-studio.mjs
```

In the viewport, select a definition, place it through the placement ghost, duplicate the selected
instance, change only its transform/material override, save, close/open, reload, and start a fresh
adapter. The accepted result retains one canonical definition per brush identity, stable
request-order owner identities, exact picking, and one shared renderer canvas. Escape cancellation,
stale project/object hashes, duplicate owners, invalid transforms, one-over aggregate bounds, and a
failed final batch entry must leave the prior project bytes unchanged. The committed reference
receipts and screenshots are `docs/evidence/voxel-brush-kit-authoring.json`,
`docs/evidence/voxel-level-brush-authoring.json`, and the adjacent
`voxel-*-studio-*.json`/`.png` files.

Neither recipe defines vertices or voxel grids in Rust gameplay logic. Neither creates a
downstream GLB decoder, asset cache, level grid, renderer, animation scheduler, or private Studio
bridge.

## Recipe: add an enemy content variant

Use this when sight/hearing range, body size, health, movement speed, attack kind/range/damage,
cadence, presentation identity, encounter membership, or deterministic drop is enough.

1. Add another entity in the canonical Studio project with a stable ID/name, explicit
   `enemyCombat`, navigation, collision/body bounds, vitality, render asset, and drop.
2. Put it in an explicit bounded encounter and define the dormant ordinary pickup that its defeat
   materializes.
3. Reference a reviewed serialized actor asset and posture binding; do not add another primitive
   fallback for shipped content.
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
combined voxel/entity occlusion query. Consume the resulting capability through the complete
adjacent Engine facade without adding revision machinery. Enemy tactics, phase order, damage
policy, and facts stay here.

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

The live owners are `door.rs`, `interaction.rs`, and `progression.rs`. Browser route navigation or
DOM visibility must never open a door, grant a key, count a secret, or complete a level.
Open an Engine task only if a second consumer needs a smaller generic collision, trigger, or
retained-transform mechanism. Access policy, item consumption, switch consequences, discovery,
completion, facts, and persistence stay here.

## Recipe: author a level variation

1. Copy a canonical project to a new stable project/scene identity and open it through Studio.
2. Change room/brush assets, responsible entity transforms/configuration, lights, pickups,
   encounter membership, or tuning. Use the same admitted schema and runtime services.
3. Keep material voxels in deterministic address order and entities in stable ID order.
4. Save/reload through Studio and run `pnpm run check:content`; require Rust admission plus exact
   byte-stable round-trip rather than equality with a TypeScript scene copy.
5. Add Rust admission/spatial checks if the variation changes collision/navigation assumptions,
   and run the real browser when the change is product visible.
6. Record original/permissive source and transformation provenance.

`content/projects/relay-annex.project.json` is the change-amplification example. It changes layout,
start/encounter placement, enemy health/navigation, weapon damage, and beacon radius through
serialized content only. The relevant implementation is localized to the canonical artifact,
semantic artifact tests, and provenance; it does not add a Rust loop, protocol, or renderer branch.
Open an Engine task only when the variation cannot be admitted by an existing game schema because
of a reusable authoring, voxel, spatial, or renderer-neutral bound already required elsewhere.
Room route, encounter composition, tuning, object names, and progression remain downstream content.

## Locality evidence

These examples report their actual change surface; none hides a runtime feature behind “content
only”:

| Variation                                               | Files changed                                         | Proof                                                                 | Owners intentionally unchanged                       |
| ------------------------------------------------------- | ----------------------------------------------------- | --------------------------------------------------------------------- | ---------------------------------------------------- |
| Add inert inspection-tag definition                     | canonical project, semantic artifact test, provenance | definition present, absent from starting stacks, Rust round-trip      | Rust services/schema, protocol, renderer             |
| Tune arc-pistol and enemy configuration for Relay Annex | `relay-annex.project.json`, semantic artifact test    | weapon definitions differ; enemy vitality/navigation differ           | combat/enemy services and phase order                |
| Rearrange Relay Annex layout and player start           | `relay-annex.project.json`, semantic artifact test    | voxel environments and player translations differ; canonical readback | Rust admission/runtime, WebSocket, Angular, renderer |

The test `settled items, weapon tuning, enemy tuning, and layout remain content-local` is the
single executable summary. If a proposed “variation” also needs schema, migration, facts, wire
decoding, or a named service, report those files honestly and follow the corresponding behavior
recipe instead.

## Forbidden shortcuts and enforced checks

| Shortcut                                                                                       | Why it is wrong                                         | Detection                                                                                       |
| ---------------------------------------------------------------------------------------------- | ------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| TypeScript inventory/combat/damage/pickup/enemy services or legacy HTTP mutators               | Creates a second gameplay authority                     | `pnpm run audit:boundary`, Rust service/failure tests, browser accepted-state proof             |
| Fake transport, placeholder actions, inert enabled controls                                    | Makes the product claim behavior it does not have       | boundary marker audit plus real Chromium menu/action proof                                      |
| Copied renderer packages, Three internals, selective Engine crate paths, or revision machinery | Forks reusable authority and breaks the complete facade | dependency/path/package and active-guidance checks in `audit:boundary`                          |
| `requestAnimationFrame` or a second `renderOnce` in downstream code                            | Creates another renderer/effect scheduler               | `audit:boundary`; shared surface lifecycle browser proof                                        |
| Voxel meshes/lights in ordinary dynamic deltas                                                 | Repeats cold resources and defeats bounded transport    | `StaticStateKey`/`RuntimeDynamicState` audit plus session tests and live static-update counters |
| Local effect/resource caches or gameplay callbacks from audio/particles/viewmodels             | Lets presentation survive/reset or mutate independently | shared host usage, projection commit tests, reload/disposal Chromium proof                      |
| Bypassing Studio/adapter for canonical project changes                                         | Skips guarded admission and canonical publication       | Studio mutation tests, `pnpm run check:content`, review                                         |

The audit catches known structural regressions; it is not permission to bypass code review with a
different spelling. When a change crosses an owner, state the new contract and failure behavior in
the task handoff.

## Verification and handoff

Use the smallest focused loop while editing, then the complete gate for any visible or
cross-language change:

```bash
pnpm run generate:content # deliberate fixtures only
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
