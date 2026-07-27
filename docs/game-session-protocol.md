# Loading Bay game-session protocol

Status: protocol version 1, implemented by Den task #6218

Loading Bay play uses one game-specific WebSocket at `/api/session` with the
`loading-bay.v1` subprotocol. Rust owns the accepted session, command validation, fixed-tick
consumption, gameplay mutation, full/delta projection, and facts. TypeScript owns browser device
capture, bounded coalescing, structural decoding, immutable projection composition, and
disposable presentation.

The protocol is deliberately downstream. It is not an Engine provider, generic message bridge,
renderer cache, gameplay RPC registry, or TypeScript simulation. The renderer remains the pinned
Rusty Engine retained surface.

## Lifecycle

1. Opening the socket starts a new Rust input connection generation and returns a full dynamic
   projection plus static resources.
2. Commands carry protocol version, session identity, a strictly increasing sequence, and the
   latest observed snapshot sequence and static-resource revision.
3. The server publishes immutable updates from the Rust fixed-tick driver, never from a second
   gameplay or render clock. Autonomous retained presentation is sampled at 30 Hz, while a newly
   consumed command publishes on that exact authoritative tick; the combined stream remains at no
   more than 60 Hz. A normal update is a delta over the immediately preceding accepted snapshot.
4. A restart loads and validates a replacement runtime before mutation, replaces the live runtime
   atomically, creates a new session identity, and sends a full dynamic projection.
5. Socket close, route disposal, or authority replacement disconnects the generation. Pending input
   and edge promises are rejected and are never replayed.

There is no automatic mutation retry. After transport loss, a fresh page or host reconnect opens a
new socket, accepts a full bootstrap, and only then may send new input. Old sequences, pending look,
held intent, and queued edges are canceled. Ordinary observed-snapshot lag is expected while one
input is in flight and does not force a full projection. A structurally rejected delta sets an
explicit one-shot `requestFullState` control envelope that is independent of the one-in-flight input
lane; an observed sequence ahead of Rust also fails closed. Rust answers that bounded control
request with a full projection even when no simulation or consumed-command sequence changed. The
observed static revision tells Rust whether that resync must include resource bytes. The browser
does not apply the non-contiguous delta, retry gameplay mutation, or settle pending input until the
full authoritative state arrives.

## Commands and bounds

Version 1 accepts this closed game-specific command family:

- `requestFullState` (internal recovery control; no gameplay mutation)
- `setInputIntent { movement, lookDelta, primaryFireHeld }`
- `interact { target }`
- `selectWeaponSlot { slot }`
- `useItem { item }`
- `setPaused { paused }`
- `restart { mode: authoredBaseline | checkpoint }`
- `saveGame { slot, overwrite, expectedStorageRevision }`
- `loadGame { slot, expectedStorageRevision }`

Continuous input is latest-wins. The browser retains one in-flight input frame and at most one
coalesced pending frame, sends at no more than 60 Hz, clamps accumulated look, and caps WebSocket
buffering at 64 KiB. Pointer-lock loss, blur, visibility loss, disposal, and restart clear pending
look and submit neutral held state.

Interaction, item use, weapon selection, pause, and restart are must-deliver edges. The browser admits at
most 32 pending edges, and Rust independently admits at most 32 queued fixed-tick edges. Saturation
rejects the new edge as `edgeQueueSaturated` without partial mutation or disturbing accepted
ordering. A numeric binding resolves to an authored zero-based slot; Rust rejects an invalid slot,
unowned weapon, already-selected weapon, unusable/missing item, full health, unavailable checkpoint,
or defeated player without changing inventory or vitality. The server
reads at most 32 commands per poll, rejects commands larger than 16 KiB, builds at most one
synchronous outbound update, caps the transport write buffer at 2 MiB, and uses a bounded write
deadline. The larger bound admits the current campaign's roughly 1 MiB cold bootstrap; ordinary
dynamic updates do not carry those static resources.

Rust retains at most 256 ordered gameplay facts before delivery. Overflow increments a visible
counter and forces a full authoritative resync. Facts in a successfully written update retain
order; presentation cues are disposable. Transport failure cancels the session instead of building
an outbound backlog.

## Static and dynamic data

The bootstrap identifies static content with
`<voxel source revision>:<voxel authority hash>`. Voxel mesh arrays, authored lights,
generated-room data, and navigation-resource hashes are sent only when that identity changes. A
session replacement with the same identity reuses the browser's immutable resource value rather
than retransmitting it.

Dynamic full and delta updates contain the Rust-owned tick, entity revision, retained projection,
door and encounter state, player/input/equipped-weapon/inventory/pickup state, player
health/armor/dead posture, authored hazard cadence, restart availability, extraction beacon,
enemies, save-slot compatibility/metadata, animation posture, and this update's facts/cues. Weapon
projection includes the selected item definition, its ammunition item and cost, the live quantity,
and cooldown eligibility.
Inventory projection exposes authored slots with owned/selected flags; it does not let TypeScript
equip or consume items locally. Deltas name their exact base and replace only changed top-level
owners. Stable `id`- or `slot`-keyed collections use a closed `$collectionPatch` shape with ordered
upserts and removals when that encoding is smaller than replacing the collection; the browser
validates identity and ordering and reconstructs the immutable whole value before projection. A
reconnect, restart, explicit gap recovery, fact overflow, or static identity change forces a full
dynamic projection; only a static identity change carries static resource bytes.

`GET /api/state` remains a read-only diagnostic snapshot and `/api/voxel-edit` remains an explicit
authoring transaction. The former per-event input, edge, disconnect, phase, beacon, and reset HTTP
mutators are unavailable.

## Failure identity

Rejections preserve a closed actionable code and retry disposition. Version 1 distinguishes
`protocolMismatch`, `sessionClosed`, `transportLost`, `staleSequence`,
`edgeQueueSaturated`, `deltaBaseUnavailable`, `invalidInput`, `unknownTarget`,
`notInteractable`, `cooldown`, `noAmmo`, `noEquippedWeapon`, `invalidWeaponSlot`,
`weaponNotOwned`, `weaponAlreadySelected`, `playerDefeated`, `itemNotOwned`, `itemNotUsable`,
`healthFull`, `checkpointUnavailable`, `saveUnavailable`, `saveOverwriteRequired`, `saveStale`,
`snapshotCorrupt`, `snapshotIncompatible`, `paused`, and `internalDefect`.
Gameplay or policy rejection does not masquerade as transport loss.

## Product proof and measurement

`pnpm run test:browser` drives realistic mouse look, held movement, weapon pickup and numeric
selection, combat, interaction, restart, resource revision changes, reload, and disposal through a
real Chromium/WebGL product. The page publishes session metrics in `data-session-*` attributes, and
the proof requires:

- held movement advances and then stops without starvation;
- client input never exceeds one in-flight plus one pending frame;
- client edges remain within 32, server outbound state remains one, and no fact overflow occurs;
- command round trips complete below the live-proof two-second failure ceiling;
- ordinary steady-state updates remain less than half the measured legacy whole-state payload;
- unchanged static resources are absent from ordinary deltas and same-revision session
  replacements.

The campaign-start loopback baseline was 98,278 bytes per whole-state action response, with 7.039 ms
average and 8.420 ms maximum sequential-request latency. That old latency excluded realistic
continuous scheduling and opened a new HTTP connection for every action. Current exact-run values
are printed by the browser proof so review evidence records payload sizes, update-build time,
bounded queue maxima, and command round-trip latency together.

The post-campaign local Chromium runs on 2026-07-26 measured a 1,015,392-byte equivalent whole
state, a 1,016,521-byte cold bootstrap, and no repeated static-resource update during ordinary
play. The largest observed steady update was 12,558 bytes. One intentionally loaded headless run
reached 858.9 ms command RTT and another reached 271.379 ms update-build time; both stayed inside
the correctness gate's two-second ceiling and completed the whole campaign. Client input peaked at
one in-flight/pending frame, client edges at one, server outbound updates at one, and dropped facts
at zero. These are loopback/headless correctness values, not interactive performance guarantees;
`docs/performance.md` owns the hardware-backed headed baseline.
