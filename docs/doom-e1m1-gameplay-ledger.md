# Doom E1M1 gameplay and location calibration ledger

Status: task #6801 campaign baseline
Parent: #6800, E1M1 living gameplay reference campaign
Calibration mode: single-player Ultra-Violence content incidence

This ledger bounds the gameplay campaign. E1M1 is the recognizable calibration target; reusable
Rust owners must not know the map name, DoomEd numbers, linedef indices, sector indices, or the
coordinates below. Doom identities, placements, difficulty flags, and tuning remain authored
project data.

Ultra-Violence is the content-incidence baseline because it is the maximal single-player E1M1 set
and the current project already admits all 29 of its enemy Things. This does not add a difficulty
system to the campaign. Easy and medium flags remain source data, and multiplayer-only Things are
outside the playable single-player scope.

## Sources and coordinate contract

The exact source and license record is in [source-provenance.md](source-provenance.md#doom-e1m1-voxel-showcase--offline-source-no-runtime-wad-den-campaign-6674).
The bounded sources used here are:

- `/home/research/doom.ts/public/doom1.wad`, id Software shareware IWAD, 4,196,020 bytes, SHA-256
  `1d7d43be501e67d927e415e0b8f3e29c3bf33075e859721816f652a526cac771`;
- `content/doom-e1m1/e1m1.intermediate.json`, the repository's ordered WAD decode;
- `ts/packages/doom-e1m1-authoring/src/{wad-decode.ts,voxelize.ts,compose-project.ts}`, the
  independently authored offline forge;
- `content/projects/doom-e1m1.project.json`, the currently admitted product content; and
- `/home/research/doom.ts/src/doom`, GPL-3.0 reading reference only. No implementation or asset
  bytes from that tree may be copied into this repository.

Objective WAD facts: 467 vertices, 475 linedefs, 648 sidedefs, 85 sectors, and 138 Things. Vertex
bounds are X `-768..3808`, Y `-4864..-2048`; sector floors span `-136..136` and ceilings reach
`264`.

The current authored transform is:

```text
world_x = (doom_x + 768) / 16
world_z = (doom_y + 4864) / 16
world_floor_surface = (doom_floor + 136) / 16 + 0.5
```

One world unit therefore represents 16 Doom units. The admitted volume is `286 x 24 x 176` cells.
Raw 8-unit floor changes become half a world unit and are currently aliased by the one-cell voxel
quantization; 16-unit stairs remain one full cell. That is a current conversion fact, not the
future traversal policy.

## Playable single-player scope

| Concern            | E1M1 calibration facts                                                                                                                                                    | Current demo at `2a0f9dc60209b1c2a780a40241ee9c9f07ff5f1b`                                                                                                                       | Reusable owner / campaign task                                                |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Start state        | One player-1 start at Doom `(1056,-3616)`, angle `90`; world anchor `(114,78)` on floor surface `9`                                                                       | Player center `[114,9.5,78]`; Arc Pistol, 30 Energy Cells, and one Med Patch                                                                                                     | Player definition, inventory, and survivability / #6804, #6805                |
| Traversal          | Route contains adjacent 8-, 16-, and 24-unit floor changes plus larger non-step height separations; four damaging nukage sectors and one repeatable lift are on the map   | Static voxel contact works over a narrow scripted route; 8-unit changes are quantized away; no grounded step/jump policy                                                         | Player locomotion and collision service / #6802                               |
| Doors and triggers | Eight type-1 boundary linedefs describe four logical manual-door sectors; one type-36 walk-once floor action; one type-88 repeatable lift action; one type-11 exit switch | Five entities are made from the first five boundary lines, rather than four logical doors. One hard-coded start-room switch controls all five. Type 36 and 88 behavior is absent | Door, trigger, lift, switch, and exit objects / #6803                         |
| Secrets            | Three sector-special-9 regions: sectors 68, 69, and 70                                                                                                                    | Only the first secret sector is authored as a live region                                                                                                                        | Secret-region owner / #6803                                                   |
| Keys               | No key Things and no keyed route requirement in E1M1                                                                                                                      | Key definitions are inherited but no key pickup is placed                                                                                                                        | No E1M1 key work; keep reusable inventory capable without surfacing keys here |
| Weapons            | Single-player set is the starting pistol plus shotgun. One shotgun Thing is placed; the 16 UV shotgun guys are an additional drop source                                  | Pistol and placed shotgun are the only authored weapon definitions and slots; multiplayer-only chaingun and rocket-launcher Things are excluded. Shotgun-guy drops remain #6807 work | Weapon definitions, equipment, firing, drops / #6804, #6807                   |
| Ammunition         | Single-player bullets and shells only: 2 clips, 1 bullet box, 2 loose shell pickups, and 3 shell boxes on UV, plus enemy drops                                            | The exact placed clip/box and shell/box counts and quantities are authored, with 50 starting bullets and eight shells on placed-shotgun collection. Enemy drops remain #6807 work   | Item definitions, pickup transaction, weapon ammo policy / #6804, #6805       |
| Health and armor   | 1 stimpack, 3 medikits, 13 health bonuses, 25 armor bonuses, 1 green armor, and 1 blue armor on UV                                                                        | All health forms collapse to Med Patches; all armor forms collapse to one unique Impact Vest                                                                                     | Pickup, bounded tracks/effects, and authored tuning / #6805                   |
| Enemies            | UV: 16 shotgun guys (type 9), 4 imps (3001), and 9 zombiemen (3004). Medium has 2 imps and 4 zombiemen                                                                    | All 29 positions exist. Types 9 and 3001 share one ranged archetype; type 3004 is incorrectly melee; no Doom-appropriate drops are authored                                      | Enemy archetypes, encounter instances, combat, drops / #6807                  |
| Hazards and props  | Four special-7 damaging nukage sectors and 6 explosive barrels (type 2035) affect play                                                                                    | Geometry and textures are present; damaging floor behavior and barrel entities are absent                                                                                        | Hazard/damage owner and encounter world objects / #6805, #6807                |
| Exit               | One type-11 exit switch at Doom midpoint `(2912,-4768)`, world `(230,6)`                                                                                                  | A radius interaction entity completes the level; it is not a switch-form interaction                                                                                             | Exit interaction and completion fact / #6803                                  |

The type-36 action is a one-shot turbo floor lower targeting sector 59. The type-88 action is a
retriggerable down-wait-up platform targeting sector 70. E1M1's stairs are static floor-height
bands, not stair-building linedef actions: representative flights use repeated 16-unit rises, with
one `0,8,16,24` band and additional 8-unit adjacency near the start-room sectors.

Source presentation also includes one flicker sector, two glow sectors, one synchronized-strobe
sector, and eight type-48 scrolling-wall lines. They are authored calibration facts, not new
gameplay authority; #6806 should reproduce them only when ordinary play shows a concrete
readability or identity gap.

Counts above exclude Things carrying the multiplayer-only flag even when they also carry all three
skill bits. Decorations, corpses, lamps, and gore do not enter gameplay scope unless later normal
play shows that a specific solid/readability role affects the route. The six explosive barrels are
included because they have combat consequences.

## Source calibration defaults

These are Doom calibration values from the GPL reference implementation, not reusable service
constants. Portable Rust owners take equivalent authored configuration and run on the Demo's fixed
tick.

| Calibration       | Source value                                                                                                                    | Current product delta                                                                                                                                                                    |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Player start      | 100 health, 0 armor, Fist and Pistol owned, Pistol equipped, 50 bullets; shells/cells/missiles 0                                | Health/armor match. Fist is absent; the player instead starts with 30 Energy Cells and one Med Patch                                                                                     |
| Body and view     | Radius 16, height 56, view height 41 Doom units; at scale 16 these are radius 1, height 3.5, eye 2.5625 world units above floor | Current kinematic half-extents are `0.25` world units on every axis. Native eye is 2 world units above the floor; browser eye is 1.7. Body and eye calibration are materially undersized |
| Ordinary step     | Maximum step-up and supported drop-off 24 Doom units / 1.5 world units                                                          | E1M1 now authors a 1.5-unit step policy; 8-unit source changes remain aliased by voxel quantization                                                                                       |
| Source clock      | 35 tics/second                                                                                                                  | Demo behavior stays on its 60 Hz fixed tick and converts authored timings; it does not reproduce a second Doom clock                                                                     |
| Manual door       | 2 Doom units/tic (70 units/s), wait 150 tics (4.286 s), destination 4 units below the lowest neighboring ceiling                | Current line-derived door entities do not preserve logical sector travel or source timing                                                                                                |
| Turbo floor lower | One shot; 4 Doom units/tic (140 units/s), geometry-dependent destination                                                        | Behavior absent                                                                                                                                                                          |
| Down-wait-up lift | Retriggerable; 4 Doom units/tic (140 units/s), 105-tic / 3 s low wait, geometry-dependent travel                                | Behavior absent                                                                                                                                                                          |
| Damaging floor    | Special 7 applies 5 damage every 32 source tics without protection                                                              | Behavior absent                                                                                                                                                                          |
| Exit switch       | Texture changes and level completion is immediate; there is no exit-delay constant                                              | Current generic radius interaction does not reproduce the switch consequence                                                                                                             |

The constants were read at `doom.ts` HEAD
`0d88ba912f7b084a05b776a19801d45f383cef20` from `src/doom/play/local.ts`,
`src/doom/game/game.ts`, `src/doom/play/map.ts`, `src/doom/play/doors/door.ts`,
`src/doom/play/floor/floor-move.ts`, `src/doom/play/plats/plat.ts`,
`src/doom/play/special.ts`, and `src/doom/play/switch.ts`.

## Stable landmark ledger

Coordinates are calibration handles for authored data and evidence only. Runtime services consume
ordinary object definitions and relationships.

| ID  | Landmark and source anchor                                                                                                                             | Objective source facts                                                                                                                                                                                            | Current product delta                                                                                                                                                                                                                                                 | Paired real-play observation                                                                                                                  |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| L1  | Start room; player Thing `(1056,-3616)`, world `(114,78)`                                                                                              | Sector 38: floor `0`, ceiling `72`, 72-unit / 4.5-world-unit clearance; floor surface `9`                                                                                                                         | Player center is `0.5` world unit above the admitted floor. Browser and native products now consume the same authored 2.0625-unit center-to-eye offset, placing the eye 2.5625 units above the source floor. The invented switch is only about 2 world units from spawn | Task #6802 bounded browser evidence records physical-control departure from `[114,9.25,78]` on authoritative surface `9`, with a retained initial world screenshot and focused gameplay canvas. Exact clean SHA and artifact index are attached to the task handoff. |
| L2  | Western green-armor court; type-2018 Thing `(-224,-3232)`, world `(34,102)`                                                                            | Sector 32: floor `128`, ceiling `264`, 136-unit / 8.5-world-unit clearance; floor surface `17`. This is an 8-world-unit elevation and major enclosure transition from L1                                          | The armor is represented as the same unique Impact Vest used for every armor Thing; the current encounter roster is present but not semantically exact                                                                                                                | The same task #6802 run arrives near `[34,17.25,102]` on authoritative surface `17` after held-key traversal across admitted surfaces `9, 11, 12, 14, 15, 16, 17`; retained before/after images show the enclosure transition, and physical Space input leaves ground and lands at L2. |
| L3  | Central nukage field; representative sector-13 point `(1824,-3276)`, world `(162,99.25)`                                                               | Sector 13: damaging special 7, floor `-80`, ceiling `216`, 296-unit / 18.5-world-unit clearance, sky ceiling, floor surface `4`; nearby route edges include 8-, 16-, and 24-unit height changes                   | Nukage is visual geometry only; sky is a clear-color opening; no floor damage or barrel gameplay exists                                                                                                                                                               | Not yet recorded. Measure walkway/step continuity, fall/recovery behavior, vertical scale, barrel readability, and enemy sight lines          |
| L4  | Eastern lift and shotgun-secret chain; type-88 line midpoint `(2900,-2964)` / world `(229.25,118.75)`, shotgun Thing `(3264,-3936)` / world `(252,58)` | Repeatable lift line is 340.35 Doom units long and targets tag 2. Secret sector 69 has floor `-48`, ceiling `32`, 80-unit / 5-world-unit clearance and contains the placed shotgun. Sectors 68-70 are all secrets | Lift behavior is absent. Only sector 68 is recorded as a secret. The shotgun substitute exists. The project also admits multiplayer-only chaingun and rocket-launcher Things elsewhere; they must leave the single-player set                                         | Not yet recorded. Measure lift timing, head clearance, secret affordance, combat pressure, weapon recognition, and route re-entry             |
| L5  | Exit annex; type-36 midpoint `(3008,-4160)` / world `(236,44)`, exit line midpoint `(2912,-4768)` / world `(230,6)`                                    | Walk-once floor action precedes a 64-unit-wide type-11 exit switch. Exit sector 82 has floor `-24`, ceiling `88`, 112-unit / 7-world-unit clearance and floor surface `7.5`                                       | The floor action is absent. The exit is a generic radius target rather than a visible switch consequence; one line-derived door entity represents only half of the nearby logical door sector                                                                         | Not yet recorded. Measure final-door and switch readability, interaction range/timing, combat-to-exit pacing, and completion feedback         |

L4's tag-2 lift target is secret sector 70; L5's tag-1 floor target is sector 59. Those relationships
belong in authored object references, never in branches on the source line or sector numbers.

The previous scripted browser route is implementation evidence only: it visited floor surfaces
`6, 7, 8, 9` and completed entity 89, but it did not measure human travel, scale, affordance, or
combat readability. It must not populate the observation column.

## Prioritized product issues

| Priority | Bounded issue                                                                                                                                 | Location                     | Owner                                                                           |
| -------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------- | ------------------------------------------------------------------------------- |
| Closed   | Multiplayer-only weapon and ammunition Things were admitted into the single-player project before #6804                                      | L4 and route-wide content    | #6804 now filters the multiplayer bit and admits only the single-player ledger |
| P0       | Player body and start inventory do not match the source baseline; eye height and the 24-unit step limit are now authored                      | L1 and all traversal         | #6804, #6805                                                                   |
| P0       | Current line-derived door entities do not represent the four logical door sectors, and the hard-coded start switch is not an E1M1 interaction | L1, L4, L5                   | #6803                                                                           |
| P0       | Damaging floors, the repeatable lift, the walk-once floor action, two secrets, and explosive barrels have no live behavior                    | L3-L5                        | #6803, #6805, #6807                                                             |
| P0       | Enemy identities, attacks, health, and drops do not preserve the three E1M1 archetypes even though all UV placements are present              | Route-wide, especially L3-L5 | #6807                                                                           |
| P1       | Health, armor, bullet, and shell pickup categories are collapsed or omitted, preventing E1M1 resource balance                                 | Route-wide                   | #6804, #6805                                                                    |
| P1       | Sixteen-unit voxels preserve 16-unit stairs but alias 8-unit floor changes                                                                    | L3 and lift approaches       | #6802; authored conversion decision only if traversal cannot preserve the route |
| P2       | Sky ceilings render as clear-color openings; this is acceptable until paired play shows a route-readability problem                           | L3 / exterior views          | #6806 only with observed evidence                                               |
| P2       | Existing native and headless screenshots are capture proofs, not normal-scale gameplay observations                                           | L1 only                      | #6806 / #6808 evidence hygiene                                                  |

P0 means the complete E1M1 route or scoped vocabulary cannot be represented. P1 means the route may
run but calibration or balance is materially wrong. P2 requires observed product impact before any
implementation work.

## Reusable capability versus authored calibration

Reusable Rust capability owns grounded movement, configurable step/jump policy, interaction range
and state transitions, inventory and bounded vitality, weapon and damage state machines, enemy
perception/attack/death/drop behavior, hazard damage, secrets, exit completion, fixed-tick order,
persistence, facts, and snapshots.

Authored E1M1 content owns WAD identities and provenance, Ultra-Violence selection, the player
start, placements, sector/line relationships, four door definitions, trigger repeatability, three
secret regions, item/enemy names and tuning, the no-key route, damage values, visual/audio
identities, and the transform/calibration values. A differently named fixture must be able to use
each reusable owner without importing any E1M1 identifier.

## Observation protocol and known uncertainty

The persistent testing CLI is not a prerequisite for the objective ledger. When reliable real
controls are available, record one short fresh session through L1-L5 with:

- exact Demo SHA, project revision, viewport, and input path;
- arrival/departure position and authoritative floor contact;
- eye height, narrowest traversed opening, representative step, and visible target distance;
- time between landmarks and interaction response time; and
- a clearly labelled human/agent observation separated from measurements and WAD facts.

That paired pass is deferred calibration evidence for later playtest work, not a prerequisite for
the task #6801 objective/source baseline.

Do not add a broad pass/fail grade. A finding enters #6806 only when it names the landmark,
reproduction, owning layer, and concrete product impact. Current uncertainties are the unrecorded
paired observation column, exact feel/timing values, and whether sky presentation is materially
harmful in ordinary play. The checked-in voxel and project provenance also carry an all-zero
`settingsSha256`; later content regeneration must replace that placeholder with the exact authored
conversion-settings identity rather than silently preserving it. The checked-in native capture
records yaw `90`, while current project authoring emits yaw `180`; treat that image as surface proof,
not heading calibration, until a fresh real-control capture reconciles the difference.
