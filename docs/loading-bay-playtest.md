# Loading Bay campaign playtest

> This playtest records the predecessor browser-rendered campaign. Task #6703 retained game and
> browser-session authority but moved rendering to the Engine-owned Rust adapter. Use
> [`design.md`](design.md) for current ownership and `pnpm run verify:native` for rendered proof.

Loading Bay is a compact original FPS route composed as immutable project data and admitted by the
same Rust runtime used by the browser product. It targets a five-to-ten-minute exploratory first
run. A direct expert route is intentionally shorter; the extra time comes from reading the space,
checking the optional storage and secret branches, learning the two enemy silhouettes, and using
the inventory rather than from slow movement.

## Authored artifact

The checked-in files under `content/projects` are the canonical Studio-owned source:

| Artifact                                    | SHA-256                                                            |
| ------------------------------------------- | ------------------------------------------------------------------ |
| `content/projects/loading-bay.project.json` | `bcbf453428ad91cb638808791c330c176f091c8659169e14c3010915594163fe` |
| `content/projects/relay-annex.project.json` | `58329c1d62a9c9c6fc002a33881adfc6e1a2779d62278f573fbcb731300eefd3` |

The schema-25 Loading Bay artifact contains one scene, 3,945 gameplay-proxy material voxels, 417
entities, 89 retained asset identities, nine item definitions, eight enemies, three encounters,
eight authored pickup caches, eight dormant defeat drops, five doors, eight lights, one secret, and
one level exit. Its visible room reuses nine canonical voxel-object definitions across 340 route
placements; the separate 25-instance proof room remains off-route, for 365 repeated instances in
all. Eight enemies use two animated identities, 25 gameplay props use serialized state bindings,
and 37 gameplay/landmark renderables are visible. Stable pretty-printing plus Rust decode,
capability-complete admission, save, and exact-byte round-trip make content drift fail the normal
verification gate. Fixture generation cannot write either canonical file.

All eight enemies plus the control panel, extraction beacon, and coolant hazard carry authored
renderable-local translations. These presentation-only offsets align their differently authored
mesh bounds to the contact plane while entity translations, collision shapes, navigation goals,
and gameplay remain unchanged. Studio reads and writes the offsets through the guarded schema-25
project contract; the presentation field itself was introduced by schema 24.

| Asset class            | Loading Bay convention                                                                                                                                | Reviewed examples                                          |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Floor-standing         | Align the transformed lower bound to the authored floor or raised-deck plane with `renderable.localTransform`; never move the entity world transform. | all enemies, doors, control, hazard, beacon, tank          |
| Wall-mounted           | Preserve the authored wall anchor and report clearance; do not apply lower-bound grounding.                                                           | level-exit marker, status runner                           |
| Suspended              | Preserve the explicit overhead support height and report clearance.                                                                                   | loading-bay crane                                          |
| Intentionally hovering | Preserve the readable pickup hover height as an explicit exception.                                                                                   | energy, health, armor, key, ammunition, and weapon pickups |

The checked [`renderable-grounding.json`](evidence/renderable-grounding.json) records every enemy
alignment and the Studio lower-bound/clearance readout for a representative door, control, pickup,
hazard, beacon, exit, crane, and tank. Its desktop and narrow captures use the shared Studio
viewport's origin, bounds, and contact-plane overlay; the convention is deliberately not a
universal `minY = 0` rule.

The browser product must compose that visual-local transform with the entity transform when it
serializes the retained surface. The previous bridge forwarded only the entity transform, so the
canonical Studio readout was correct while the same meshes floated in the game. The checked
[`renderable-pivot-product.json`](evidence/renderable-pivot-product.json) pairs the human-reachable
**Tools > Pivot & Grounding** workflow with a same-view live browser before/after. This is the
acceptance evidence; the focused Rust regression only prevents the projection path from dropping
the visual-local transform again.

The wall-alignment descendant keeps that same authority split. Supported Rust voxel editing added
14 missing material voxels beneath six decorative columns, while the repeated-brush recipe removed
the overlapping wall presentation at those footprints, aligned both southern corner accents to
the `z=49` boundary, and composes each doorway from collision-backed wall jambs plus a decorative
header whose cells remain above walk height. A global interval audit rejects any walk-height
presentation surface not covered by Rust material-voxel collision, including surfaces belonging to
a different adjacent opening.
The checked [`wall-proxy-alignment.json`](evidence/wall-proxy-alignment.json) compares 267 walls,
six columns, four corners, five door headers, and the one-unit generator passage. Its measured
maximum horizontal visual-to-proxy gap is zero at six-decimal evidence precision, below the
smallest authored brush cell of `1/32` world unit. Material voxels remain the only collision,
navigation, and occlusion authority.

Relay Annex changes the room arrangement, player start, initial enemy placement and tuning,
navigation target/speed, and beacon radius through serialized project data. It uses the same Rust
services, host loop, protocol, renderer, and browser shell; there is no variant-specific gameplay
loop.

Bay Rusher melee remains meaningful but leaves room for real delayed input: an accepted strike
deals eight damage on a 120-tick cadence. The campaign proof uses the same bounded held-input path
as physical keys and consumes authored med patches through Rust during a prolonged fight; it does
not grant invulnerability, disable damage, or mutate vitality from the browser.

## Route checkpoints

1. **Arrival floor.** Start with the arc pistol, 18 energy cells, and one med patch. Collect the
   visible energy cache and defeat the first Bay Rusher to open the cargo pressure door.
2. **Side storage.** The straight generator route is visibly gated. The optional west branch gives
   shells, a med patch, and the breach scattergun. Its sealed manifest recess records the one
   secret and contains the impact vest.
3. **Generator floor.** Fight a mixed three-enemy group around a coolant hazard. The room supplies
   two med patches, the rivet carbine, and the maintenance pass. Defeat drops materialize exactly
   once as health or energy ammunition.
4. **Maintenance loopback.** Return to the earlier maintenance bulkhead, open it with the retained
   pass, and use the generator interlock. The same Rust switch operation closes the generator door
   and opens the extraction gate, making the changed route legible in the world.
5. **Extraction approach.** Follow the opened gantry route past the moving status runner and
   extraction beacon. The player now has three weapons with distinct ammunition and firing modes.
6. **Dock encounter.** Crossing the dock threshold activates two Bay Rushers and two Arc Wardens.
   Closed entity doors participate in canonical occlusion for both player and enemy rays.
7. **Exit.** After the encounter clears, activate the level exit. The completion dialog permits a
   completed save, and Continue restores that exact completed Rust snapshot in a fresh host
   process.

## Product proof checklist

The real Chromium gate starts from the main menu with a fresh save root and drives physical
keyboard, pointer-look, primary-fire, weapon-selection, interaction, item-use, and focus behavior
through the browser shell. It requires all of the following before reporting success:

- all three encounters activate and clear, all eight enemies are defeated, and all eight dormant
  drops materialize;
- the sidearm, spread weapon, and automatic weapon each produce their distinct accepted attack cue;
- pickup quantities, health use, armor, key ownership, secret discovery, door/interlock state,
  beacon activation, and level completion come from authoritative session projections;
- renderer timing reports animation-frame cadence and backend submission duration from the one
  shared `RendererSurface`, with bounded input, edge, fact, snapshot, and outbound queues;
- death/restart focus, narrow and desktop overlays, route disposal/remount, resize, reset, and
  viewmodel exclusion from picking remain intact;
- a fresh browser and host begin a new authored runtime, while Continue against the isolated
  campaign save root restores the completed slot and completion dialog;
- a separate stored-project round trip preserves live voxel edits, and the converted-asset and
  schema-6 migration products still load through their supported compatibility paths.

The automated route is a correctness and lifecycle proof, not a human-duration benchmark. A manual
playtest should record start-to-exit time separately and note optional branch use, deaths, remaining
health/armor, and ammunition at each checkpoint. Any tuning change belongs in immutable project
content; it must not add browser-owned combat or progression state.

## Complete-product certification

The visual-content campaign was recertified on 2026-07-30 at exact public Demo revision
`67e8d1d609f46d11fe8da0d990fc7a9b6ab33285`, pinning exact public Engine revision
`0e0c49442d0c3d876a1336a5a829087f6e2314db`. Exact GitHub verify run
`30595962674` / job `91048265860` passed with authoritative RTT max 1536.6 ms, bounded queues
1/1/1, zero dropped facts, and every campaign, completed-save, fresh-host Continue, converted
project, migration, fresh-page, statistics, picking, reset, remount, and disposal tail. The run
kept the full 89-asset/419-entity/367-instance scene, normal held controls, damage/resources, and
the unchanged two-second retained-transport budget.

The final desktop and narrow screenshots are
`docs/evidence/final-game-desktop.png` and `docs/evidence/final-game-narrow.png`; the associated
Studio actor, prop, and repeated-brush screenshots and stale/conflict/lifecycle receipts are indexed
by `docs/evidence/final-visual-content-certification.json`.

### Pre-visual-campaign certification

The campaign was certified on 2026-07-27 from the public runtime revision
`e31dea511377fe68ab898248c5ee9efa3f9a2cf6`. A fresh managed host built the Angular product, loaded
the checked-in Loading Bay project, and served the real browser shell at `http://127.0.0.1:8787/`.
Cargo and pnpm resolved only the exact public Engine revisions recorded in
`docs/source-provenance.md`; no sibling checkout, path override, global package link, copied
renderer, or browser-owned gameplay substitute participated.

The real Chromium proof completed the route with normal held movement, pointer look, firing,
numeric weapon selection, interaction, and item use. It covered all pickups and weapon/ammunition
families, both enemy archetypes, real damage and death/restart, the maintenance pass, interlock,
secret, extraction beacon, level exit, completed save, and exact completed-state restoration in a
fresh host session. The same run checked desktop and narrow layouts, pause and dead dialogs,
pointer-lock loss/recovery, reconnect/error presentation, synthesized audio scheduling, renderer
resize/reset/disposal/remount, and zero retained presentation timers or effects after cleanup.

The representative headed 20-second movement/look workload used Chromium 148 on a Radeon 780M at
1600x900 with real pointer lock, held forward input, and CDP mouse motion. It observed 35 retained
entities and eight resident chunks:

| Measure                                      | p50    | p95    | p99    | Maximum |
| -------------------------------------------- | ------ | ------ | ------ | ------- |
| Shared-renderer submission cadence (ms)      | 16.7   | 16.8   | 16.8   | 16.8    |
| Synchronous backend submission time (ms)     | 0.4    | 0.6    | 0.8    | 0.8     |
| Authoritative snapshot cadence (ms)          | 33.2   | 41.4   | 42.2   | 43.8    |
| Input command acknowledgement latency (ms)   | 12.997 | 45.283 | 46.340 | 48.036  |
| Ordinary dynamic session payload size (byte) | 2,291  | 2,394  | 2,684  | 2,686   |

The proof correlated 505 actual input command sequences with their authoritative consumed-command
updates, sampled 199 renderer frames, observed input/edge/outbound queue maxima of 2/1/1, and
reported zero dropped facts. Renderer cadence comes from the shared `RendererSurface`; backend
submission duration remains a separate synchronous measurement and is not presented as GPU
completion time. `docs/performance.md` owns the complete method, budgets, and raw context.

Scope accounting for this certification:

- All #6214 campaign acceptance criteria are exercised by the immutable content checks, focused
  Rust and browser-shell suites, production build, complete real-browser campaign, exact headed
  performance proof, and repository-wide `pnpm run verify` gate.
- The accepted navigation footprint constraint is fail-closed and does not prevent the shipped
  route. It remains documented in the project `known-limitations` record rather than hidden behind
  a second navigation authority.
- Desktop startup and steady-state profiling (#6292) now owns the former #6293 bootstrap question:
  non-game routes are lazy, the Rust menu summary no longer transfers the full game projection, and
  the measured one-time game bootstrap remains accepted under its explicit bound. An independent
  human pacing playtest (#6294) remains separate tuning work rather than an authoritative campaign
  correctness gap.
- Source and asset provenance remains exact in `docs/source-provenance.md`; the FPS campaign uses
  original Loading Bay composition and serialized Kenney CC0/original Loading Bay presentation
  rather than licensed Doom content.
