# Loading Bay design and authority boundary

Loading Bay is one Rusty Engine downstream game. Its supported content is the authored Doom E1M1 project; retired fixture projects are not alternate products.

## Authority

`LoadingBayProductService` is the game-specific, transport-neutral Rust authority. It admits the authored project, builds and advances the fixed-step loop, accepts typed semantic commands, owns session generations, saves, facts, and projection readouts. It knows no socket, Tauri, or renderer implementation.

Rust owns runtime state, validation, mutation, scheduling, persistence, and projection. TypeScript owns immutable content composition, browser semantic-input capture, and disposable HUD/product-shell presentation. It does not become a second gameplay evaluator.

### Authored gameplay programs

The E1M1 gameplay package and canonical project carry small, family-specific
closed program catalogs. TypeScript composes immutable trees or flat sequences
from each family's named predicates and operations; Rust compiles them once at
project admission and executes them through the owning service's candidate
transaction. Facts, evidence, inventory mutation, damage, scheduling, and
events remain Rust-owned. Program catalog readouts describe that admitted
composition; they neither replay it nor give TypeScript a runtime evaluator.

Pickup collection is a separate, family-local catalog. Each placement binds a
closed pickup program; its only predicate is whether that pickup's starter
weapon is already owned, and its only operations grant the placement's item or
starter ammunition, apply that granted health/armor item, or consume the
pickup. Rust traverses the authored tree in source order inside the existing
candidate-session/trigger transaction. Enemy defeat may materialize a dormant
drop, but never runs its pickup collection program.

Player initialization is a third, independent family: a flat source-ordered
sequence of `grantItem` and `equipInitialWeapon`. Admission resolves the
selected `inventory.setupProgram` into Rust-owned inventory facts, collision-safe
reserved weapon identities, and owned weapon materialization before a session
exists. The owner inventory is attached first; the Engine atomically admits an
owned unique weapon, attaches its item fact, and establishes containment, then
the separate initial-equipment operation plans against those fresh facts.
Unowned weapon slots reserve an identity but admit no hidden Engine entity or
item component. First pickup materializes that reserved identity in the existing
product candidate, while a previously disposed item is explicitly recontained
without rematerialization. An inventory always names its setup program.
Snapshots persist whether each reserved identity is materialized; older
snapshots with pre-admitted hidden placeholders remain readable, and restore
never reruns setup.

Hazards and explosive props are two further closed families. Each hazard binds
`playerOverlapping`, `playerEligible`, and `cooldownReady` around
`applyHazardDamage` and optional `scheduleHazardCooldown`; each explosive prop
binds `explosionPending` around radial target selection, scaled damage, and
resolution. Rust executes both in source order against a candidate phase, so a
failed later operation cannot commit damage, cooldowns, trigger revisions, or
prop state. Static catalog/binding readouts are descriptive only; these
automatic environmental runs never overwrite the player-action outcome.

Switch interaction is another closed family. A switch program may gate on
`switchAvailable`, record activation, request open/close only for the switch's
already-admitted bound door effects, and emit interaction feedback. Programs
never carry door IDs or selectors. Rust owns actor/range/repeatability checks,
door motion/collision/auto-close scheduling, and commits session, scheduler,
events, and journal together only after the full program succeeds.

Floor actions and lift cycles are separate closed walk-trigger families rather
than a general state-machine language. Their programs may select source order
over activation feedback/request and phase-specific lower, wait, and raise
primitives; the only target remains the component's admitted `targetPlatform`.
Rust captures the component state at the beginning of each motion invocation,
so a program cannot chain a newly reached phase in one pass. Rust also owns
the WAD-derived translations/durations, trigger reconciliation, collision,
facts, and the one candidate transaction spanning the floor and lift phases.

Encounters are a closed lifecycle family with distinct typed activation and
clear trees. `activationEligible` may order activation recording, the explicit
encounter members' Rust-owned readiness cadence, and feedback;
`membersDefeated` may order clear recording and an optional already-admitted
exit-door request. Programs carry neither member nor door IDs. Rust owns
spatial admission, member lifecycle, door motion and scheduling, facts, and
event order. Event draining evaluates clear programs on a candidate
session/scheduler/queue/journal, so a late authored error cannot leak an
encounter state, exit transition, event, or journal entry; an upstream enemy
defeat remains the separate committed damage consequence.

Secrets and level exits complete the same pattern with their own tiny families:
secret entry/once/discovery/presentation and exit availability/completion. Their
programs contain no region, entity, or selector language. Rust owns WAD-derived
regions and IDs, overlap and range checks, once-only state, mutation, facts,
and presentation events.

### Repeating the pattern downstream

This is a downstream authoring pattern, not a portable behavior language.
Another game first defines a small sealed Rust vocabulary of responsible
predicates, operations, facts, and service transaction. It then exposes
matching immutable TypeScript builders, compiles named family programs at
admission, binds them to already-admitted game objects, and executes them in
Rust against a candidate state before committing. Values and object references
remain in authored components; behavior order lives only in the relevant
family's program.

Rusty Dagger may use richer RPG-specific vocabulary where that product needs
it. Loading Bay deliberately does not inherit that grammar, nor does it define
a universal Engine behavior IR. A future downstream should author its own
bounded vocabulary rather than import Dagger semantics or add a registry,
selector language, generic bridge, or live TypeScript authority.

### Standard gameplay adoption

Loading Bay uses the public `gameplay-standard` actor and destructible catalog
fragments as ordinary `gameplay-mechanics` definitions. The Demo visibly
composes those configured fragments with its armor, inventory, and damage
definitions; the presets do not create a world, registry, scheduler, or hidden
evaluator. The
actor vitality track is used by player/enemy health; explosive props select the
standard destructible integrity track through the same Doom damage, health
projection, and snapshot paths. The compatibility 50-point capacity remains
for current E1M1 content, while a future valid prop explicitly raises the
admitted standard catalog bound rather than being rejected or locally
reconstructed. Health/armor policy, damage sources, hitboxes, pickups, enemy
defeat/drop behavior, and explosive consequences remain named Doom adapters in
Rust.

`gameplay/authoring/src/packages/e1m1-standard-vitality.ts` is the companion
generated-TypeScript DSL example. It authors one narrow `loading-bay.vitality`
extension containing E1M1's cap policy. `LoadingBayProductService` admits and
compiles that canonical package before constructing the runtime, then passes
its typed health/armor bounds through normal gameplay admission. The extension
is deliberately not mixed into Doom's closed encounter/pickup/program trees:
those remain product-specific typed Rust vocabularies.

The developer-command client is a host surface, not gameplay authority. When a
product adapter exposes it, it must call a product-owned command queue at a
selected fixed-step safe point, obtain receipts from existing mechanics owners,
and keep discovery, profiles, and schemas explicit. Loading Bay does not add a
generic HTTP mutation endpoint or console-local health state for convenience.
Its `LoadingBayProductService` exposes Engine's borrowed safe-point bindings for
standard entity/mechanics inspection, standard track mutation, and one typed
Loading Bay play command. Browser development uses a separate bounded
`/api/developer-command` WebSocket that cannot create or disconnect the game
session; Tauri uses typed discover/submit/poll/cancel IPC while the existing
desktop ticker reaches the same safe point. The product TypeScript adapter
selects those transports and injects the generated public client into the
Engine application-host console. A request is unavailable without an active
gameplay generation, cancellation drops only queued work, and a Loading Bay
play result is published only after its ordinary service outcome is observed.
Session replacement or disconnect converts every queued or in-flight request
into an immediate `retired-generation` result retained for transport polling;
callers never wait for their adapter timeout to discover retirement.
The runtime identity remains stable for the admitted project while discovery's
revision advances with each gameplay generation, so the long-lived application
client can reacquire the live context without mistaking an ordinary restart for
a different product runtime.

## Adapters

`browser-host` is the normal development adapter: it translates bounded `loading-bay.v2` WebSocket messages to the service and exposes read-only diagnostics. The Angular shell consumes the projected content through Rusty Engine's public application-host.

Tauri is the final-product adapter. It packages the same shell in one WebView and calls typed in-process commands over the same service. It has no packaged browser-host process, loopback readiness protocol, asset-hash handshake, orphan process cleanup, or second product window.

Rusty Engine alone owns renderer and canvas lifetime. This repository neither imports a private bridge nor builds a competing renderer/resource cache/frame loop.

## Dependency and extension boundary

The only Rust provider dependency is the complete adjacent `rusty-engine` facade, through one unconditional path dependency. Owner namespaces remain explicit. This repo does not pin or manage the sibling checkout, and it does not select Engine subcrates.

Game-owned semantics belong here first. Promote a neutral Engine seam whenever
one concrete product need, proof, or architecture decision shows that shared
ownership prevents duplicate authority or correctness drift; consumer count is
not a gate. Do not introduce a plugin registry, service locator, generic
method-name bridge, behavior IR, replay/certification framework, or live
TypeScript authority.

## Content and evidence

`content/projects/doom-e1m1.project.json` is the sole canonical project. E1M1 textures, sprites, voxel data, and the eight-prop closure live under `content/doom-e1m1/`; exact source boundaries are in [source-provenance.md](source-provenance.md). Historical campaign material belongs in Den, not as active repository documentation or proof.

Project admission accepts only the current stored-project schema. Loading Bay
does not reconstruct predecessor content or install inferred behavior during
migration; older experiments must be re-authored through the current immutable
content builders.
