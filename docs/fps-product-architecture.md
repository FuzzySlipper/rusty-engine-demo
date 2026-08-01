# Loading Bay FPS product architecture

Status: campaign contract for Den parent #6214
Baseline audited: 2026-07-26 at repository head
`a695cb8dc0f37ae4684266d5b500d8b891f6f3c5`

This document is the durable implementation contract for turning the proof-oriented Loading Bay
host into a compact single-player FPS. It describes the intended end state. A statement in this
document is not evidence that the current product already implements it; the current-state
inventory and task references make that distinction explicit.

The product is an original game. It borrows the useful progression vocabulary of a compact
classic-FPS level, not another game's code, map, geometry, names, sounds, textures, sprites, or
trade dress.

## Campaign implementation inventory

| Surface              | Current implementation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | Product gap and owner                                                                                                   |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| Rust runtime         | `LoadingBayGameLoop` owns a fixed 60 Hz phase order over `GameRuntime`: input, player motion, bounded encounter activation, enemy intent/motion, hazards, combat, interactions/pickups, scheduled consequences, host operations, and projection/fact drain. The pinned Engine gameplay-mechanics provider owns canonical inventory quantities, unique-item containment, equipment, health/armor tracks, armor effects, damage, and healing. Loading Bay named services own game-specific ordering, weapons/ammunition policy, hazards, progression, enemy behavior, defeat drops, save/load, and consequences. | New gameplay semantics extend a responsible component and named phase/service here, not a browser callback or plugin.   |
| Player input         | The browser captures devices and submits bounded latest-wins continuous intent plus must-deliver edges. Rust owns connection generation, sequence/ack, expiry, pause, dead-state rejection, and fixed-tick consumption. Look work is one in-flight plus one coalesced pending frame. Host-user sensitivity and invert-Y transform only device deltas before that bounded submission.                                                                                                                         | Later tasks may extend authored bindings, but no accepted gameplay intent moves into TypeScript.                        |
| Browser host         | One Loading Bay-specific WebSocket session loads the admitted authored baseline, publishes bounded full/delta updates, persists bounded save slots at the command-consumption tick, and replaces the runtime atomically after fixed-tick restart/load receipts. Legacy HTTP gameplay mutators are absent.                                                                                                                                                                                                    | The downstream session and save store remain game-specific; no generic Engine RPC bridge is planned.                    |
| Transport payload    | Static render resources travel by content revision; ordinary updates carry bounded dynamic owners and facts. Player vitality, hazards, inventory, restart availability, enemy combat posture/attack kind, and dead posture are immutable Rust projections.                                                                                                                                                                                                                                                   | Later feature tasks extend concrete owners; no generic method bridge or TypeScript gameplay state is planned.           |
| Renderer             | `RuntimeProjectionAdapter` maps game state to shared Engine render descriptors. `mountRendererSurface` owns the retained Three/WebGL surface and its only animation scheduler. Exact-pinned Engine #6263 adds a bounded camera-relative `viewmodel` layer composed after world depth; shared hosts own audio, particles, billboards, and telemetry.                                                                                                                                                          | No private renderer substitute exists; later work may add assets through the same renderer-neutral retained mechanisms. |
| Browser presentation | The Angular/Nx shell provides a working main menu, session-bound Continue flow, authoritative HUD/hotbar, live inventory, Rust pause, save/load, host settings, visible typed rejections, and responsive overlays around one shared auto-started renderer surface. Three original primitive weapon silhouettes, bob, recoil, and muzzle flash are disposable descriptor state derived from accepted Rust projection/facts. Its diagnostic drawer reports renderer cadence and backend submission separately. | Presentation can add disposable polish through typed projection, but cannot retain live gameplay authority.             |
| Content              | TypeScript composes immutable schema-19 project JSON for the complete original Loading Bay route and Relay Annex variation. Responsible objects own vitality, hazards, weapons, keys/doors/switches/secrets/exits, concrete melee/ranged enemy combat configuration, bounded encounter activation, and defeat-drop relationships. Rust alone owns all live quantities, progression, combat, damage, death, drops, and mutation facts. Legacy schemas reject future behavior fields before migration.         | New level arrangements stay in immutable composition unless they require a genuinely new game-owned semantic.           |
| Live persistence     | `GameSnapshot` schema 19 round-trips the Engine component store for canonical mechanics plus collision, doors, switches, enemy combat posture/memory/cooldowns, encounter activation, defeat-drop state, hazards, navigation, controllers, explicit weapon policy/cooldowns, item definitions, checked health/inventory projections, pickups, progression, schedules, and tick. Schema 10–18 saves are migrated into the canonical component store; schema-19 projections must exactly agree with it. The bounded save store persists compatible snapshots and metadata; `ProjectStore` separately persists authored project content. | Connections, derived routes/queries, static render resources, and presentation state are deliberately not persisted.    |
| Verification         | Focused Rust integration tests cover each current owner. TypeScript tests cover composition, queueing, projection, and feedback. `pnpm run verify` adds boundary audits, exact dependency checks, build, full Rust checks, and a real Chromium/WebGL product smoke.                                                                                                                                                                                                                                          | Every campaign task adds focused proof. #6233 certifies a complete fresh-process playthrough and performance budgets.   |
| Known limitations    | Den document `rusty-engine-demo/known-limitations` owns active limitations.                                                                                                                                                                                                                                                                                                                                                                                                                                  | Update the same entry when each limitation is resolved; a task thread or code comment alone is insufficient.            |

### Measured proof-shaped baseline

The campaign-start host was measured on loopback with the admitted Loading Bay project and its
current generated mesh:

- `GET /api/state`: 98,278 response bytes and 7.555 ms total request time.
- Twenty sequential `look` actions: 98,298 average response bytes, 7.039 ms average round trip,
  8.420 ms maximum.
- Each action used a separate `Connection: close` request and returned unchanged static mesh arrays.

This is diagnostic evidence from one local run, not a cross-machine benchmark. It establishes why
the current path is unacceptable for continuous input and supplies the comparison point for #6218.

## Authority map

| Concern                                                             | Sole authority                                        | Consumers                         |
| ------------------------------------------------------------------- | ----------------------------------------------------- | --------------------------------- |
| Authored project, immutable definitions, original level composition | TypeScript composition admitted and validated by Rust | Project tooling, Rust admission   |
| Fixed tick, phase order, accepted time                              | Loading Bay Rust game loop                            | Services, projection, persistence |
| Player/world/enemy state and mutation                               | Responsible Rust components and named services        | Snapshots and read-only views     |
| Input intent acceptance, sequence/ack, expiry, pause                | Loading Bay Rust session                              | Browser projection                |
| Inventory quantities, item containment, equipment, tracks/effects  | Pinned Engine gameplay-mechanics provider             | Loading Bay policy and projections |
| Weapons, ammunition policy, pickups, death, restart, consequences  | Loading Bay Rust                                      | HUD and disposable feedback       |
| Doors, switches, keys, secrets, exit                                | Loading Bay Rust                                      | World projection and UI           |
| Enemy perception, intent, motion, attack, drops                     | Loading Bay Rust                                      | World projection and cues         |
| Runtime snapshots, checkpoints, saves                               | Loading Bay Rust                                      | Host storage boundary             |
| Browser keyboard, mouse, pointer-lock, focus capture                | TypeScript host edge                                  | Typed command encoder             |
| Session connection, bounded batching, reconnect                     | Loading Bay-specific Rust/TypeScript transport edges  | Game session                      |
| Angular routes, view models, accessibility, loading/error UI        | TypeScript presentation                               | User                              |
| Mouse sensitivity, invert-Y, audio, HUD visibility                  | Host-user TypeScript settings                         | Input transform and presentation  |
| Retained render resources, surface lifecycle, timing, Three/WebGL   | Exact-revision shared Rusty Engine renderer           | Angular canvas host and telemetry |
| Camera/HUD/audio/particles/viewmodel effects                        | Disposable presentation derived from Rust state/facts | User                              |

TypeScript must not become a second simulation, inventory/equipment store, aim authority, pickup
owner, enemy AI, save owner, render-resource cache, effect simulation, or animation scheduler.
Rust must not absorb browser device APIs, Angular behavior, or host-user preferences.

## Target execution sequence

### Bootstrap

1. TypeScript content composition emits immutable project bytes and provenance.
2. `ProjectStore` reads canonical bytes; Rust decodes, validates, and admits every definition and
   relationship.
3. Rust creates the authored baseline and a distinct live `GameSession`.
4. The host publishes a content manifest containing the project revision and hashes for static
   geometry and shared render resources.
5. The browser loads each cold resource once by identity, mounts one shared renderer surface, and
   opens one game-session connection.

### Fixed simulation tick

The Loading Bay game-loop owner runs a fixed 60 Hz simulation. One tick has this explicit order:

1. **Input consumption:** accept the newest valid continuous intent and bounded edge commands;
   expire stale or disconnected intent.
2. **Player motion:** integrate velocity and apply authoritative collision.
3. **Enemy intent and motion:** derive perception/attack intent, then move selected enemies through
   the same spatial authority.
4. **Hazards:** reconcile authored trigger volumes and apply due damage through the same vitality
   owner; a lethal result clears held player intent before any later mutation.
5. **Combat:** validate cooldown, ownership, ammunition, aim, occlusion, damage, armor, death, and
   deterministic drops.
6. **Interactions and pickups:** apply item grants, switches, doors, secrets, and exit transitions
   transactionally.
7. **Scheduled consequences:** drain due, bounded game-specific consequences such as door close.
8. **Projection and fact drain:** produce one immutable dynamic projection plus ordered facts/cues
   for this tick.

The host may run at most five catch-up ticks after a delay. It does not accumulate an unbounded
time debt. A larger discontinuity is reported and the game resumes from the current monotonic time.
No component callback, browser timer, renderer callback, or plugin scheduler can advance gameplay.

### Browser consumption

1. The transport applies a full resync or a contiguous delta and acknowledges the associated input
   sequence.
2. Angular derives immutable view models for HUD/menu/inventory/session states.
3. The game projection submits changed descriptors to the retained shared renderer.
4. The renderer owns frame submission and reports its latest timing sample.
5. Presentation consumes facts once for audio, particles, recoil, damage flash, and messages.
6. Normal gameplay derives a first-person camera at the accepted player X/Z and exactly 1.2 world
   units above the accepted player Y. It has no horizontal follow/trailing offset, so camera and
   collision cannot straddle a wall or doorway boundary.
7. Any pending-look camera offset is bounded, disposable, reconciled on acknowledgement, and
   cleared on rejection, reconnect, pause, death, pointer-lock loss, or route disposal. Rust pose
   remains the only aim and firing authority.

### Persistence and reset

- Authored project bytes remain distinct from live runtime snapshots.
- A save/checkpoint records semantically durable live state and definition/content revisions.
- Held keys, pending look, network state, telemetry history, audio, particles, and other
  presentation state never enter a snapshot.
- New Game rebuilds the admitted authored baseline. Restart restores the selected checkpoint or
  baseline through a typed Rust receipt. Load validates schema and content compatibility before
  replacing live state atomically.

## Game-session contract

The wire format is versioned and game-specific. Rust types are canonical; generated or
structurally checked TypeScript types may encode/decode them. These stable shapes guide #6217,
#6218, and later feature tasks. The implemented version-1 lifecycle, bounds, cancellation rules,
and live measurement proof are recorded in [`game-session-protocol.md`](game-session-protocol.md).

### Client command envelope

```text
ClientCommandEnvelope {
  protocolVersion: 1
  sessionId: string
  sequence: u64
  observedSnapshotSequence?: u64
  observedStaticRevision?: string
  command: GameCommand
}
```

`sequence` is strictly increasing within one connection generation. Repeated sequences are
idempotently acknowledged when their prior result is still retained; older unknown sequences are
rejected as stale. A reconnect creates a new connection generation and begins with a full resync.

`GameCommand` is a closed tagged union:

- `setInputIntent { movement: [f32; 2], lookDelta: [f32; 2], primaryFireHeld: bool }`
- `selectWeaponSlot { slot: u8 }`
- `useItem { item: ItemDefinitionId }`
- `interact { target?: EntityId }`
- `setPaused { paused: bool }`
- `restart { mode: authoredBaseline | checkpoint }`
- `saveGame { slot: SaveSlotId }`
- `loadGame { slot: SaveSlotId }`

Movement and fire-held are latest-wins state. Look deltas are coalesced only until the next accepted
tick and remain bounded. Weapon selection, interaction, pause transitions, restart, save, and load
are must-deliver edges in a bounded queue. Feature tasks may add another concrete command only when
it has a named Rust owner; there is no generic method, payload, or behavior bridge.

### Server update envelope

```text
ServerUpdateEnvelope {
  protocolVersion: 1
  sessionId: string
  serverTick: u64
  snapshotSequence: u64
  acknowledgedCommandSequence: u64
  staticRevision: ContentRevision
  update: FullProjection | DynamicDelta
  facts: [GameFact]
  cues: [PresentationCue]
  metrics: SessionMetrics
}
```

A full projection follows bootstrap, reconnect, an observed sequence gap, incompatible delta base,
or bounded fact retention overflow. A dynamic delta names its base and changed owners. Both are
immutable views; neither exposes mutable Rust component storage. Facts have stable identity and
ordering. Cues are disposable and may be dropped without changing gameplay.

### Rejection envelope

```text
CommandRejection {
  protocolVersion: 1
  sessionId?: string
  commandSequence?: u64
  acknowledgedCommandSequence: u64
  code: RejectionCode
  retry: never | reconnect | resync
  message: string
  details?: typed bounded data
}
```

The closed `RejectionCode` family distinguishes:

- `protocolMismatch`, `sessionClosed`, `transportLost`
- `staleSequence`, `edgeQueueSaturated`, `deltaBaseUnavailable`, `contentRevisionMismatch`
- `invalidInput`, `unknownTarget`, `notInteractable`
- `weaponNotOwned`, `weaponAlreadySelected`, `invalidWeaponSlot`, `noEquippedWeapon`, `noAmmo`,
  `cooldown`
- `inventoryFull`, `itemUnavailable`, `accessDenied`
- `playerDefeated`, `paused`, `levelComplete`
- `saveUnavailable`, `snapshotIncompatible`
- `internalDefect`

An ordinary policy or gameplay rejection is not a transport failure. `edgeQueueSaturated` means
the 32-command must-deliver edge queue was already at capacity: the new edge was not accepted or
enqueued, every previously accepted edge remains ordered, and continuous latest-wins input remains
independent. A failed or rejected command does not partially mutate state, consume ammunition,
duplicate a pickup, or block later valid commands.

### Queue, cancellation, and stale-state rules

- The client retains one latest continuous input frame and at most 32 must-deliver edge commands.
- The server retains one latest outbound state update and at most 256 ordered must-deliver
  fact/rejection records per session. Overflow forces a typed full resync; it never grows memory.
- At most two simulation ticks of unacknowledged look may affect disposable camera presentation.
- Blur, pointer-lock loss, route disposal, pause, death, disconnect, and session replacement
  immediately clear client-held input and cause Rust intent to expire no later than two ticks.
- The client never retries a stale mutation automatically. Reconnect performs a handshake and full
  resync before new commands.
- Restart/load/session replacement cancels queued work from the previous session generation.

## Cold, dynamic, and transient data

| Class                       | Examples                                                                                                                                                                                                                                                 | Transport and persistence                                               |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| Cold/static content         | Project identity/schema, level geometry, voxel chunks, materials, meshes, item/weapon/enemy definitions, pickup and door configuration, presentation asset identities, default bindings                                                                  | Loaded once by revision/content hash; authored project persistence only |
| Durable live state          | Tick, entity transforms, inventory quantities, equipped weapon, ammo pools, cooldowns, health/armor, enemy state, pickup availability, doors/switches, keys, secret/exit state, checkpoint metadata, deterministic spread state when semantically needed | Dynamic full/delta projection; included in Rust runtime saves           |
| Session-transient authority | Accepted held input, input sequence/ack state, connection generation, queued edge commands                                                                                                                                                               | Bounded in Rust session; never saved                                    |
| Disposable presentation     | Pending-look offset, interpolation history, recoil/bob, particles, audio, damage flash, message timers, telemetry chart history                                                                                                                          | Browser-only and rebuildable; never authoritative or saved              |

Static mesh arrays must not appear in an ordinary input acknowledgement or simulation delta when
their content identity is unchanged.

## Minimum content vocabulary

Stable definition identity is a namespaced string admitted by Rust. The initial content set is:

- `weapon/arc-pistol`: reliable single-shot sidearm using `ammo/energy-cell`.
- `weapon/breach-scattergun`: bounded deterministic spread weapon using `ammo/scatter-shell`.
- `weapon/rivet-carbine`: held automatic weapon using `ammo/energy-cell`.
- `ammo/energy-cell` and `ammo/scatter-shell`: explicit bounded inventory quantities.
- `key/maintenance-pass`: retained access-key ownership.
- `supply/med-patch`: bounded health restoration.
- `armor/impact-vest`: bounded armor/protection grant.
- `enemy/cargo-loader`: short-range pursuing attacker.
- `enemy/gantry-sentry`: ranged attacker with explicit perception and attack cadence.

Definitions are responsible data, not entries in a behavior registry. Adding an inert content-only
item requires no Rust behavior change; adding a genuinely new weapon or enemy behavior requires a
concrete Rust owner and tests.

## Original level route

The authored Loading Bay route is compact, readable, and looped:

1. **Arrival cage:** teach movement, pointer lock, the arc pistol, and energy-cell pickup.
2. **Cargo floor:** introduce one cargo loader, health, cover, and the first readable locked door.
3. **Side storage:** optional branch containing scatter shells, the breach scattergun, and the
   discoverable secret.
4. **Generator bay:** mixed loader/sentry encounter, armor, rivet carbine, and maintenance pass.
5. **Return gantry:** a shortcut returns the player to the locked door and demonstrates retained key
   ownership.
6. **Control room:** a deliberate switch changes the exit route and starts the final encounter.
7. **Extraction dock:** the final fight, checkpoint opportunity, and authoritative exit completion.

The route proves weapon and ammo pickups, numeric selection of owned weapons, distinct ammunition,
health/armor, key denial and success, switch consequences, a secret, death/restart, checkpoint
restore, and completion without requiring copied content or a generalized quest system.

## Product budgets

Performance certification records machine, OS, browser build, viewport, renderer backend, exact
Demo SHA, and exact Engine pins. Headless SwiftShader smoke is correctness evidence, not the
interactive performance machine.

| Concern                | Required budget                                                                                               |
| ---------------------- | ------------------------------------------------------------------------------------------------------------- |
| Simulation             | Fixed 60 Hz; no more than five catch-up ticks; no browser-event-driven advancement                            |
| Continuous input       | At most 60 submitted input frames/second, one latest pending frame, no promise tail                           |
| Edge commands          | At most 32 queued; the next edge is rejected as `edgeQueueSaturated`, with no silent loss or partial mutation |
| Input acknowledgement  | Local-LAN p95 at most 50 ms and no sample above 100 ms during the 60-second stress route                      |
| Steady dynamic payload | p95 at most 4 KiB/update during ordinary movement/look                                                        |
| Full dynamic resync    | At most 32 KiB for the campaign level, excluding separately hashed static resources                           |
| Static resources       | Zero unchanged mesh/voxel array bytes in ordinary dynamic updates                                             |
| Server outbound state  | One latest unsent state plus 256 must-deliver records maximum                                                 |
| Renderer cadence       | Interactive p95 at most 20 ms and p99 at most 33.5 ms during the representative route                         |
| Submission duration    | Report separately from cadence; never label synchronous backend submission as GPU time                        |
| Reconnect              | Full resync and safe idle intent within two seconds after the host becomes reachable                          |
| Long-run stability     | 10-minute stress route has no growing input, fact, cue, snapshot, or render-resource count                    |

Task #6218 records before/after traffic and latency. Task #6219 records real
`surface.timing()` cadence and backend submission duration from upstream #6213. Task #6233 records
the complete playthrough evidence.

## Acceptance corpus

Every task keeps focused tests close to the owner and uses `pnpm run verify` for product-visible or
cross-language changes.

| Proof                 | Required evidence                                                                                                                                                                                              |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Architecture boundary | `pnpm run audit:boundary`; exact public Git pins; no sibling path, private renderer, generic bridge, or TypeScript gameplay store                                                                              |
| Fixed loop            | Deterministic Rust tests for cadence independence, phase order, catch-up bound, stale intent, disconnect, pause, death, and restart                                                                            |
| Protocol              | Codec tests, sequence/ack/idempotence tests, queue overflow yielding exactly `edgeQueueSaturated` with no silent loss or partial mutation, reconnect/full resync, typed failures, payload-size instrumentation |
| Inventory/combat      | Atomic grant/consume/equip/fire tests, weapon ownership, distinct ammo, cooldown, spread determinism, death and reopen                                                                                         |
| World progression     | Pickup idempotence, key denial/success, switch/door scheduling, secret first-discovery, checkpoint, exit completion                                                                                            |
| Renderer/presentation | Shared-surface lifecycle, no double loop, resource retention, real timing, disposable feedback reset                                                                                                           |
| Browser product       | Real Chromium tests for menu, pointer lock, held input, numeric weapons, HUD/inventory, pause/reconnect, death/restart, save/load                                                                              |
| Campaign              | Fresh managed-browser playthrough of all seven route beats within budgets and with current provenance/limitations                                                                                              |

## Upstream promotion gate

Before implementing a missing mechanism:

1. Inspect current Den guidance/tasks and the exact public Engine package surface.
2. Classify the need as Loading Bay semantics or a smaller reusable provider/renderer mechanism.
3. For upstream ownership, open/link a task in project `rusty-engine`, stop the local substitute,
   and make the consumer task depend on the reviewed public revision.
4. Pin the exact public revision and run the complete downstream proof. Never make a sibling path
   the supported build.

The two proven upstream gaps were renderer-owned timing (Rusty Engine #6213, public SHA
`2665b74566136fb77e3a26b0766394124c8f58d3`) and a reusable retained camera-relative viewmodel
channel (Rusty Engine #6263, public SHA `e622c941671bc0f167206b049ab94ea63495a86d`). The fixed game
loop, input-intent semantics, Loading Bay session transport, items, inventory, weapons, enemies,
progression, save policy, and level remain downstream. No evidence presently justifies an Engine
plugin API, scheduler, service registry, behavior IR, method bridge, private renderer, or
replay/certification spine.
