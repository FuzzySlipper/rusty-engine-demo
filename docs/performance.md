# Loading Bay performance evidence

> Historical browser-renderer measurements: task #6703 removed the downstream browser renderer.
> These captures remain predecessor provenance. Current rendered-host acceptance is
> `pnpm run verify:native`; [`design.md`](design.md) owns the active boundary.

The Loading Bay product uses one `autoStart: true` Rusty Engine renderer surface. Browser input and
authoritative session snapshots update that retained surface; the demo does not schedule another
render loop or call `renderOnce()` alongside the automatic loop.

## Counter ownership

The shared renderer panel reads one immutable `surface.timing()` sample:

- `frameTimeMs` is inter-submission cadence from the renderer's frame source timestamp.
- `backendSubmissionDurationMs` is synchronous host time spent submitting to the backend. It is
  CPU-side submission evidence and does not measure GPU completion.
- entity, resident-chunk, and render-diff counts describe retained presentation work. Draw-call and
  GPU timings are not exposed by the current public surface and are not inferred downstream.

The separate game-session panel reports Rust/server tick, accepted snapshot sequence and arrival
cadence, current dynamic payload bytes, bounded input and edge queues, and command round-trip time.
These counters are diagnostic projections only; they do not schedule or mutate gameplay.

## Interactive budgets

The representative route is ordinary first-person look plus held WASD movement through the loading
bay at a 1600 by 900 viewport:

| Measurement                    |                                        Budget |
| ------------------------------ | --------------------------------------------: |
| Renderer cadence               |                  p95 <= 20 ms; p99 <= 33.5 ms |
| Synchronous backend submission |        Reported separately; no GPU-time claim |
| Continuous input               | <= 1 in flight plus 1 coalesced pending frame |
| Edge input                     |                        <= 32 pending commands |
| Dynamic update payload         |                                  p95 <= 4 KiB |
| Local command acknowledgement  |               p95 <= 50 ms; maximum <= 100 ms |
| Simulation                     |   Rust-owned fixed 60 Hz; <= 5 catch-up ticks |

Performance certification records OS/kernel, CPU/GPU, browser build, viewport, exact Demo revision,
and exact Engine pins. The automated Chromium gate intentionally uses headless SwiftShader and
proves lifecycle/correctness only; it is not interactive GPU performance evidence.

## 2026-07-26 managed LAN baseline

The headed product ran from the `den-serve` managed LAN URL for 20 seconds of pointer-locked look
and held WASD input. This table records the single certification run attached to the #6219 review
request (task message 24922) at exact Demo revision
`6a4cacf1f130d0b818bb727369c6a8aeb69bf391`; renderer packages were pinned to Engine
`2665b74566136fb77e3a26b0766394124c8f58d3`.

| Context        | Value                                                                 |
| -------------- | --------------------------------------------------------------------- |
| Host           | EndeavourOS; Linux 7.0.11; AMD Ryzen 7 8845HS; 16 logical CPUs        |
| GPU/backend    | AMD Radeon 780M; ANGLE OpenGL ES 3.2; Mesa 26.1.2                     |
| Browser        | Chromium 148.0.7778.215                                               |
| Viewport/route | 1600 x 900 product canvas; loading-bay pointer-lock route             |
| Exercise       | 20 seconds; 193 unique renderer samples; 193 authoritative look facts |

| Measurement                         |   p50 |   p95 |   p99 |   max |
| ----------------------------------- | ----: | ----: | ----: | ----: |
| Renderer cadence (ms)               |  16.7 |  16.8 |  16.8 |  16.8 |
| Synchronous backend submission (ms) |   0.2 |   0.3 |   0.5 |   0.7 |
| Snapshot arrival cadence (ms)       |  19.8 |  24.3 |  27.2 |  40.4 |
| Command acknowledgement (ms)        |  17.2 |  54.3 |  59.0 |  60.8 |
| Dynamic payload (bytes)             | 2,474 | 3,103 | 3,115 | 3,117 |

The renderer and payload budgets pass. Command acknowledgement remains below the 100 ms maximum but
misses the 50 ms p95 target by 4.3 ms; this is why the bounded local look presentation offset is
enabled. The observed offset peaked at 0.30 yaw units and 0.10 pitch units, stayed below its
two-unit-per-axis bound, and reconciled to exactly zero. Input evidence stayed within two pending
frames, pointer lock remained active, and the browser reported no runtime error.

## 2026-07-27 complete-campaign headed certification

`pnpm run certify:performance` is the repeatable headed-hardware proof. It opens the installed
product in a visible Chromium/Wayland window, acquires real pointer lock, holds physical `W`, sends
CDP mouse motion, reads only shared-surface timing, and correlates each WebSocket input sequence
with the Rust update that both acknowledges and consumes it. The RTT percentile is therefore over
505 distinct commands rather than repeated samples of the HUD's last-value diagnostic.

The following exact run used Demo revision
`e31dea511377fe68ab898248c5ee9efa3f9a2cf6`, Rust Engine revision
`464dd5e16bb023ad8d81515eabeaac9bb75df74d`, and renderer package revision
`e622c941671bc0f167206b049ab94ea63495a86d`.

| Context        | Value                                                           |
| -------------- | --------------------------------------------------------------- |
| Host           | Arch Linux; Linux 7.0.11; AMD Ryzen 7 8845HS; 16 logical CPUs   |
| GPU/backend    | AMD Radeon 780M; ANGLE OpenGL ES 3.2                            |
| Browser        | Chromium 148.0.7778.215                                         |
| Viewport/route | 1600 x 900 product canvas; pointer-locked held movement/look    |
| Exercise       | 20 seconds; 199 renderer samples; 505 acknowledged input frames |

| Measurement                         |   p50 |   p95 |   p99 |   max |
| ----------------------------------- | ----: | ----: | ----: | ----: |
| Renderer cadence (ms)               |  16.7 |  16.8 |  16.8 |  16.8 |
| Synchronous backend submission (ms) |   0.4 |   0.6 |   0.8 |   0.8 |
| Snapshot arrival cadence (ms)       |  33.2 |  41.4 |  42.2 |  43.8 |
| Command acknowledgement (ms)        | 13.00 | 45.28 | 46.34 | 48.04 |
| Dynamic payload (bytes)             | 2,291 | 2,394 | 2,684 | 2,686 |

The shared renderer reported 35 retained entities, eight resident chunks, zero to three render
diffs per sampled state, and animation-frame timing throughout. Input, edge, and outbound maxima
were 2, 1, and 1 respectively; dropped facts remained zero. All interactive budgets pass.
Autonomous retained state is projected from every second 60 Hz Rust tick, while a newly consumed
command publishes on its exact tick. This keeps gameplay authority and scheduling in the one Rust
loop, holds autonomous presentation at 30 Hz, and avoids adding a browser or renderer clock.

## 2026-07-27 desktop-representative startup profile

`pnpm run profile:desktop` is the repeatable startup and steady-state profiler. It creates a
production build with Angular build statistics, starts a fresh Rust host on an isolated port and
save root, and opens that product in a visible hardware-accelerated Chromium/Wayland window. The
profile records:

- cold and warm navigation to a rendered menu whose Rust-owned Continue availability has settled;
- resource transfer, long tasks, JavaScript heap, whole Chromium process-tree resident memory,
  DOM nodes, and listeners at the menu and in gameplay;
- two-second idle deltas for task/script/layout/style work, heap, nodes, and listeners;
- New Game activation through the first authoritative projection and shared-renderer frame;
- input event to next animation frame, authoritative consumed-command RTT, renderer cadence, and
  synchronous backend submission time;
- initial versus lazy JavaScript and module attribution from Angular's build graph.

This historical measurement predates the Tauri package and remains a Chromium baseline, not a
native-shell launch, installer-size, or process-memory certification. The current Tauri build and
WebKit lifecycle are documented in `docs/tauri-desktop.md`; DT2 records the comparable installed
native measurements instead of retroactively relabeling these Chromium numbers.

The pre-change build at exact revision
`648a28bb84b245bcca974b1826655af35a52eef9` eagerly included the game screen, browser session,
presentation hosts, Rusty Engine renderer, Three.js, settings, and diagnostics at the main menu:
1,120,493 raw JavaScript bytes and a 1.14 MB initial build total. The menu also fetched the complete
approximately 1,014,538-byte `/api/state` projection merely to decide whether Continue was
available. A focused pre-endpoint diagnostic run, used to justify the change rather than as the
certification revision, measured 1,256,803 transferred bytes and 336.833 ms to a usable cold menu.

The implementation now uses real Angular lazy route boundaries and a Rust-owned
`GET /api/menu-state` summary containing only the host continuity identity and four save-slot
summaries. The complete game projection remains authoritative and is still loaded when the player
enters the game. The exact post-change run used Demo revision
`687593d9283b5d27eee1b6e4055d18ca354692c6`.

| Context          | Value                                                                      |
| ---------------- | -------------------------------------------------------------------------- |
| Host             | Arch Linux; Linux 7.0.11; AMD Ryzen 7 8845HS; 16 logical CPUs              |
| GPU/backend      | AMD Radeon 780M; ANGLE OpenGL ES 3.2                                       |
| Browser          | Chromium 148.0.7778.215                                                    |
| Viewport         | 1600 x 900 visible Wayland window                                          |
| Cold/warm policy | New temporary browser profile; same process reload for the warm menu       |
| Runtime policy   | Fresh Rust host, fresh save root, real pointer lock, five seconds of input |

| Startup measurement                     |   Cold menu |   Warm menu |    Gameplay |
| --------------------------------------- | ----------: | ----------: | ----------: |
| Usable wall time (ms)                   |     166.090 |      70.248 |     490.800 |
| Encoded resource bytes                  |     239,830 |     239,820 |           — |
| Encoded JavaScript bytes                |     223,538 |     223,538 |           — |
| JavaScript heap used (bytes)            |   2,447,216 |   3,511,580 |  11,505,376 |
| Chromium process-tree resident bytes    | 826,712,064 | 937,099,264 | 994,201,600 |
| Long-task count / maximum duration (ms) |       0 / 0 |       0 / 0 |     1 / 161 |
| DOM nodes / registered event listeners  |      74 / 9 |    141 / 24 |    314 / 45 |

The resident-memory number includes Chromium's browser, renderer, GPU, network, and utility
processes. It is deliberately reported rather than interpreted as Loading Bay's native packaged
footprint. The warm snapshot is taken without forcing garbage collection, so it also reflects
ordinary retained browser-process state.

| Two-second idle delta              | Cold menu | Warm menu | Gameplay |
| ---------------------------------- | --------: | --------: | -------: |
| Total task duration (ms)           |     7.976 |     2.953 |  226.853 |
| Script duration (ms)               |     0.000 |     0.000 |  105.419 |
| Layout / style recalculation count |     1 / 1 |     0 / 1 |  20 / 20 |
| Node / listener delta              |     0 / 0 |     0 / 0 |  101 / 0 |

The menu has no continuing script work or listener growth in either sample. Gameplay work is
expected: the one Rust-owned fixed loop publishes retained state and the one shared surface renders
it. No second scheduler was added.

| Input measurement                       |   p50 |   p95 |   p99 |   max |
| --------------------------------------- | ----: | ----: | ----: | ----: |
| Input event to next frame (ms)          |   0.9 |   1.6 |   2.4 |   2.5 |
| Authoritative consumed-command RTT (ms) | 18.39 | 46.26 | 47.21 | 47.56 |
| Renderer cadence (ms)                   |  16.7 |  16.8 |  33.4 |  33.4 |
| Synchronous backend submission (ms)     |   0.4 |   0.7 |   1.0 |   1.0 |

The exact run collected 141 input-to-frame samples, 114 authoritative RTT samples, and 85 unique
shared-renderer timing samples. Pointer lock remained active and no runtime error was present.

### JavaScript and bootstrap attribution

| Phase/category             |   Raw bytes |
| -------------------------- | ----------: |
| Initial Angular            |     198,918 |
| Initial other third-party  |      17,682 |
| Initial Loading Bay        |       4,526 |
| Initial demo libraries     |       1,291 |
| **Initial total**          | **223,538** |
| Lazy game: Three.js        |     609,822 |
| Lazy game: Engine renderer |     157,715 |
| Lazy game: browser runtime |      84,256 |
| Lazy game: Loading Bay     |      28,292 |
| Lazy game: demo libraries  |      14,147 |
| **Lazy game total**        | **895,229** |
| All JavaScript             |   1,122,058 |

The total application code is essentially unchanged; the optimization removes unrelated parse and
initialization work from the menu rather than chasing download size. The configured 1.10 MB initial
warning now passes naturally, so it was neither raised nor converted into a product success metric.

The first game session still reports a 1,016,523-byte bootstrap and a 1,015,393-byte equivalent
whole projection. Its largest observed server build was 146,977 microseconds, the first
authoritative rendered frame arrived in 490.8 ms, and ordinary updates reported zero static-resource
retransmissions. This is accepted for the supported local desktop direction: it is bounded at
2 MiB, paid once on game entry, and did not produce a measured interaction or startup failure that
would justify a second resource authority or speculative Engine API. A future packaged-shell
profile can reopen resource delivery work if those measurements change.

## 2026-07-28 visual-content placeholder baseline

Den task #6351 refreshed the same desktop and managed-LAN routes at exact Demo revision
`cd25485445bfb581c4005b221a23caa21408d327`, before campaign #6350 replaces the placeholder
geometry. The complete inventory, source shortlist, authority migration, and comparison method are
in [`docs/visual-content-pipeline.md`](visual-content-pipeline.md). Structured raw evidence is in
[`docs/evidence/visual-content-placeholder-baseline.json`](evidence/visual-content-placeholder-baseline.json).

The fresh-host desktop profile measured 105.656 ms to the cold menu, 66.296 ms to the warm menu,
494.4 ms through the first authoritative projection and shared-renderer frame, a 1,016,524-byte
session bootstrap, and 11,553,112 bytes of gameplay JavaScript heap. The 20-second managed-LAN run
measured:

| Measurement                         |    p50 |    p95 |    p99 |    max |
| ----------------------------------- | -----: | -----: | -----: | -----: |
| Renderer cadence (ms)               |   16.7 |   16.8 |   33.3 |   33.4 |
| Synchronous backend submission (ms) |    0.4 |    0.7 |    0.9 |    1.0 |
| Snapshot arrival cadence (ms)       |   31.6 |   41.1 |   41.9 |   41.9 |
| Command acknowledgement (ms)        | 18.523 | 47.286 | 48.254 | 49.267 |
| Dynamic payload (bytes)             |  2,124 |  2,393 |  2,739 |  2,752 |

The shared telemetry held 35 projected entities and eight resident voxel chunks, with zero dropped
facts and queue maxima 2/1/1. The Engine revision pinned for that historical run did not expose
renderer-owned draw, geometry, material, texture, or animated-instance counts. Those values remain
recorded as unavailable rather than being reconstructed after the fact.

The active Wayland output reported 59.951 Hz during automation, so the observed 16.7 ms cadence is
refresh synchronization. The monitor EDID also supports 119.989 and 144 Hz; an approximately 8.4 ms
cadence in a 120 Hz session has the same meaning and is not a render-duration measurement.

## 2026-07-28 renderer statistics follow-up

Rusty Engine #6361 introduced an immutable renderer-neutral statistics sample, and downstream
#6378 adopted it at exact Engine revision
`a6857d03141e162511231c276ee751a3413c90e5`. The exact tested Demo implementation is
`602e8ed60312aaea308097abb9816b8523a5bd1f`; raw evidence is in
[`docs/evidence/renderer-statistics-certification.json`](evidence/renderer-statistics-certification.json).

One real Chromium/SwiftShader run used only the ordinary Loading Bay `RendererSurface`. It captured
the placeholder, added 32 visible instances sharing four static resources under one temporary
viewmodel tree, captured the richer load, removed the root, and captured the restored surface. The
probe submitted explicit frames but created no surface, frame loop, timer, gameplay mutation, or
Three/WebGL/private-object access.

| Renderer statistic  | Scope          | Placeholder | Rich stress | Delta | Restored |
| ------------------- | -------------- | ----------: | ----------: | ----: | -------: |
| Draw calls          | per submission |          39 |          71 |   +32 |       39 |
| Live handles        | live resident  |          51 |          84 |   +33 |       51 |
| Geometry resources  | live resident  |          43 |          47 |    +4 |       43 |
| Material resources  | live resident  |          55 |          59 |    +4 |       55 |
| Texture resources   | live resident  |           0 |           0 |     0 |        0 |
| Animated instances  | live resident  |           0 |           0 |     0 |        0 |
| Submitted triangles | per submission |      14,380 |      14,444 |   +64 |   14,380 |

Every observation was `available`; zero therefore means exact zero. `perSubmission` counters reset
before the combined world/viewmodel render. `liveResident` counters describe backend-owned state
after that submission, not authored entities or cumulative allocations. The three synchronous
submission durations were 0.6, 8.3, and 1.0 ms under headless SwiftShader and are lifecycle evidence,
not desktop GPU performance measurements.

The ordinary telemetry path now consumes `RendererSurface.submission()`. `profile:desktop` retains
the latest complete sample, while `certify:performance` reports status/scope plus available-value
ranges and fails if the exact Three counters disappear or change scope. Frame cadence remains
inter-submission timing and synchronous backend submission remains distinct from GPU completion.

## 2026-07-30 content-rich moving-camera compaction follow-up

Task #6354 now measures the unchanged serialized prop/viewmodel and 342-placement voxel-brush
product at exact Engine revision `51281becc482fd71c0f3b2be16d9abee6a37b5be`. The shared
renderer retains bounded definition-compatible static candidates, compacts each submission to the
current camera frustum, and admits at most one latest automatic demand while the preceding WebGL2
GPU stream is incomplete. Every logical handle, transform, metadata record, refcount, and picking
identity remains authoritative and available. No content, downstream cache, render loop, gameplay
authority, campaign assertion, fixed-rate cap, or performance budget was removed.

The predecessor `c903c1c86761386087acd7d7d814a3da5cde116b` reduced draw calls but formed
scene-wide batches with culling disabled. Exact CI run `30532095039` consequently expired after
Generator with a 7,506.8 ms maximum command RTT. That result is retained as the rejected
intermediate rather than being represented by the faster workstation sample. The first culling
revision, `e97944c8309018f595222edb7bd90a620c32cedf`, used 32-unit cells; this real project placed
367 instances into only three cells (222 / 126 / 19), and exact CI run `30535751200` still expired
after Generator at 9,152.7 ms maximum RTT. The later 8-unit
`6fe4713df76ce0a03a6c461dfa95d4a90b24c824` revision formed 129
cell-and-definition groups, matching the broad 131-draw diagnostic submission. Exact CI run
`30538613245` reached Loopback but still expired at 300 seconds with 6,899.0 ms maximum RTT. Those
results are retained as rejected intermediates rather than being represented by faster workstation
samples.

The current provider retains the definition-compatible moving-camera compaction and bounds
automatic submission by actual WebGL2 GPU completion. Unsupported, lost, or failed sync falls open
with bounded cleanup, while explicit `renderOnce` and reset remain unconditional. The table below
is the unchanged full four-core local campaign and lifecycle run; exact-SHA CI remains the task
review gate rather than a value inferred from this table:

| Measurement                         | Result      |
| ----------------------------------- | ----------- |
| Session bootstrap                   | 1,159,686 B |
| Largest steady update               | 11,912 B    |
| Outbound / input / edge queue peaks | 1 / 1 / 1   |
| Dropped facts                       | 0           |
| Maximum update build                | 266,889 µs  |
| Maximum authoritative command RTT   | 339.9 ms    |
| Renderer cadence                    | 16.6 ms     |
| Rich draw-call delta                | +32         |
| Rich live-handle delta              | +33         |
| Rich submitted-triangle delta       | +64         |

The explicit statistics probe reports 40 draws for 412 live handles and 63,227 submitted
triangles. Its 32-instance stress frame reaches 72 draws, 445 handles, and 63,291 triangles, then
cleanup returns to 40 / 412 / 63,227. Geometry and material counts remain at 48 / 86 rather than
the 44 / 82 pre-stress baseline because Engine #6416 deliberately retains four reusable
static-mesh definitions after their last live instances are destroyed. Reuse is bounded, and
renderer disposal remains the terminal release boundary.

Exact Demo revision `53de81f29813e13ceb710929c42fb7a3072a7f48` disproved that draw-group
compaction alone closes the supported CI profile. GitHub run `30542408476` reached Arrival,
Storage, and the locked door, then expired at the unchanged 300-second campaign deadline with an
11,146.5 ms maximum command RTT. Queues remained bounded at 1 / 1 / 1 and no facts were dropped,
but one input remained pending and the later campaign, save, fresh-host, and lifecycle evidence was
never reached.

A matching four-core local run (`taskset -c 0-3 pnpm run test:browser`) on the compaction-only
provider completed gameplay, progression, checkpoint, and save but failed the unchanged transport
budget at 2,755.9 ms maximum RTT. During that run the SwiftShader GPU process consumed approximately
304% CPU, the Chromium renderer 51%, and the Rust host 38%, saturating the four assigned cores. A
later mutation-demand provider reduced the failure to 2,479.3 ms but still could not observe when
the GPU command stream was complete.

The exact current provider closes that ownership gap with renderer-owned WebGL2 completion
backpressure. On the same four-core command, the unchanged campaign, completed save, fresh-page
restore, converted asset, picking, resize/reset/remount, and disposal evidence all completed with
339.9 ms maximum RTT. Queues remained 1 / 1 / 1 with zero drops. The displayed 16.6 ms cadence and
synchronous backend duration remain submission diagnostics rather than GPU-duration claims; the
completion fence is used only to bound automatic demand. The demo adds no private scheduler,
coalescer, or test-only degraded scene.

That local result did not close the supported exact CI profile. Demo revision
`8f1c451c6df9fe5dded5b433d82a2918500e7b11` pinned the completion-fenced provider, but GitHub run
`30548042802` again exhausted the unchanged 300-second watchdog after Arrival, Storage, and the
locked door. Terminal command RTT reached 6,638.1 ms, one input remained pending, queues remained
bounded, no facts were dropped, and Generator plus all later campaign/persistence/lifecycle
evidence was absent. The completion fence is retained as valid bounded renderer behavior, but its
local improvement is not represented as product acceptance. Engine #6434 continues to own the
remaining generic workload correction.

Engine revision `3077798ae70bc7cb6c54fab5fb50f43766dd3b56` first closed that constrained
profile locally without reducing the retained scene or weakening the transport budget. The
unchanged two-core browser campaign completed the full route, checkpoint, completed-save, reload,
converted-project, and lifecycle tails with 1,488.5 ms maximum command RTT. Exact Demo revision
`9ad855f343661c0bbeee5ec4c4c380cc3da72b1d` then completed the same route and persistence tail in
GitHub run `30560832289`, but its 2,354.2 ms maximum RTT exceeded the retained 2-second bound.
Queues remained 1/1/1 with zero dropped facts and zero pending input, so this was a residual
software-raster workload failure rather than transport loss.

Exact Engine revision `7119f6d78725ee2363fac7424d150e5f1735ccf1` lowers only positively
identified software-renderer backing buffers from a 0.5 to a 0.375 pixel-ratio ceiling. That is
43.75% fewer backing pixels while preserving CSS dimensions, camera projection, normalized
picking, lower requested ratios, and every accelerated or unknown renderer's requested ratio. Its
unchanged two-core campaign passed locally, but exact Demo revision
`54a4192e33239c24633440718f309035bed9b9d4` failed the retained transport assertion in GitHub run
`30564003751`: the complete route, checkpoint, completed save, and renderer statistics passed, but
maximum RTT was 2,156.4 ms and snapshot cadence was 520.8 ms. Queues remained 1/1/1 with zero
dropped facts and zero pending input. The aggregate therefore stopped before the remaining
fresh-host, converted-project, and lifecycle tails, so that revision was not accepted.

The current correction is exact Engine revision
`8fae5fb770a73baa3bec259a6b71cf12ed3de5e6`. It lowers only the positively identified
software-renderer backing-buffer ceiling from 0.375 to 0.25, reducing raster area by a further
55.6% while preserving the same CSS, camera, picking, lower-ratio, and accelerated/unknown
renderer boundaries. The unchanged two-core campaign completed the full route, checkpoint,
completed slot-3 save, fresh-host reopen, converted-project and v6 migration, renderer statistics,
picking, resize/reset/remount, disposal, and fresh-page posture tails with 1,139.3 ms maximum RTT,
166.7 ms automatic submission cadence, queues bounded at 1/1/1, zero drops, and zero pending input.
The scene remains 9 definitions, 42,266 authored cells, and 342 placements; no downstream renderer
scheduler, cache, content reduction, timeout, budget, or alternate acceptance path was added.

## 2026-07-30 VC9 content-rich desktop profile

`pnpm run profile:desktop` now records the renderer-owned automatic-submission pacing sample beside
the existing submission timing and statistics. Set
`RUSTY_ENGINE_DEMO_PROFILE_OUTPUT=docs/evidence/content-rich-desktop-profile.json` to retain the
machine-readable report. The profiler still creates a production build, an isolated fresh Rust
host and save root, and one visible hardware-backed Chromium/Wayland surface. It neither creates a
second renderer nor reaches into Three or WebGL. Set
`RUSTY_ENGINE_DEMO_PROFILE_CPU=true` only for a diagnostic V8 sampling profile; CPU sampling is
disabled for certification runs so the observer cannot perturb the measured input path.

The first retained report is exact Demo revision
`646cc89db70e8f1499e809152cfcd1cc4c485c14`, pinned to Engine
`80ac6ed3f0bd1d9911edf44e33bcc90831d8909e`. The machine is the same Ryzen 7 8845HS / Radeon 780M
desktop used by the placeholder baseline, with Chromium 148, a 1600 by 900 viewport, and the active
Wayland output at 59.951 Hz. The EDID also exposes 119.989 and 144 Hz. A displayed 16.7, 8.4, or
6.9 ms interval therefore identifies 60, 120, or 144 Hz presentation cadence; it is not a GPU
duration claim.

### Three measured scene stages

| Stage                | Exact Demo revision                        | Serialized content                                                                                                                                      | Representative result                                                                                                              |
| -------------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Placeholder baseline | `cd25485445bfb581c4005b221a23caa21408d327` | 47 authored entities, 35 renderables, coarse hidden-compatible voxel environment, no voxel-object definitions                                           | 494.4 ms first authoritative frame; 1,016,524 B bootstrap; 11,553,112 B gameplay JS heap; 16.8 ms p95 cadence                      |
| Brush proof room     | `030f45f78bad10cbb85ebbcb50eeaedba727bb56` | 9 definitions, 25 instances, 42,266 stored cells; both 32×32×4 conservative and 64×64×8 dense wall treatments                                           | 2,450,553 B Studio project; 139,584.899 ms for the historical sequence of 25 singular attachments; 5,886.366 ms maximum attachment |
| Complete product     | `db25b6b95a0d1bc3d39ae8aca2aa4fecae898123` | Same 9 definitions reused by 342 route placements plus the 25-instance proof room, 8 animated gameplay actors, and 25 serialized gameplay prop bindings | 3,425 ms first authoritative frame; 1,183,676 B bootstrap; 61,812,420 B gameplay JS heap; 45 draws, 415 handles, 68,201 triangles  |

The proof-room timing is retained as an intentionally expensive predecessor, not the supported
authoring workflow. Protocol 12 publishes the 342 route placements in 11 atomic batches of at most 32. The batches total 80,050.110 ms, the slowest is 7,956.815 ms, and the complete structural
projection is 739,471 bytes. Singular attachment is not looped downstream.

### Stress matrix

| Dimension                   |                  Low / baseline |                                  Product |                                     High proof | Owning evidence                                            |
| --------------------------- | ------------------------------: | ---------------------------------------: | ---------------------------------------------: | ---------------------------------------------------------- |
| Repeated brush instances    |                               0 |                     342 route + 25 proof |                 4,097 typed one-over rejection | `voxel-level-brush-authoring.json`; `studio_voxel_objects` |
| Voxel detail per definition | 2,228 cells, 219.531 ms prepare | 42,266 cells across 9 shared definitions |     16,386-cell dense wall, 881.433 ms prepare | `voxel-brush-kit-authoring.json`                           |
| Animated instances          |                               0 |                                        8 | 12, covering both identities and all six clips | `actor-kit-studio-browser.json`                            |
| Prop/static stress          |          Placeholder primitives |             25 bound props and landmarks |  Temporary +32 instances sharing 4 definitions | `renderer-statistics-certification.json`                   |

The exact-limit and one-over tests run through Rust project/Studio admission before publication.
They retain the accepted project bytes and private candidate on rejection. These are capacity and
atomicity proofs; the high rows are not alternate degraded gameplay scenes.

### Hardware measurements and budgets

The following budgets are the VC9 desktop target. Historical interaction targets remain unchanged:
renderer p95 <= 20 ms and p99 <= 33.5 ms; authoritative command acknowledgement p95 <= 50 ms and
maximum <= 100 ms. A red row is retained as a real gap rather than relaxed to fit the sample.

| Measurement                              |                Budget |        Exact result |  Status  |
| ---------------------------------------- | --------------------: | ------------------: | :------: |
| Cold / warm usable menu                  |       <= 500 / 250 ms | 156.346 / 69.011 ms |   pass   |
| First authoritative projection and frame |           <= 5,000 ms |            3,425 ms |   pass   |
| Session bootstrap                        |              <= 2 MiB |         1,183,676 B |   pass   |
| Gameplay JavaScript heap                 |             <= 96 MiB |        61,812,420 B |   pass   |
| Chromium process-tree RSS                |           <= 1.25 GiB |     1,102,725,120 B |   pass   |
| Input event to next frame p95 / max      |          <= 8 / 16 ms |        3.5 / 4.2 ms |   pass   |
| Authoritative command RTT p95 / max      |        <= 50 / 100 ms |  52.958 / 55.598 ms | **fail** |
| Synchronous backend submission p95       |               <= 8 ms |              2.6 ms |   pass   |
| Accelerated timer-query GPU work p95     |               <= 8 ms |             6.62 ms |   pass   |
| Renderer cadence p95 / p99               |       <= 20 / 33.5 ms |      16.8 / 16.8 ms |   pass   |
| Draws / live handles / triangles         | <= 64 / 512 / 100,000 |   45 / 415 / 68,201 |   pass   |
| Geometry / material / texture resources  |       <= 64 / 128 / 8 |         38 / 97 / 2 |   pass   |
| Animated instances                       |                 <= 16 |                   8 |   pass   |
| Two-second gameplay idle listener growth |                     0 |                   0 |   pass   |

The initial retained checkpoint showed that the content itself was not saturating the hardware
GPU. Timer-query work was p50 4.661 ms and p95 6.711 ms, while synchronous submission was p50
1.5 ms and p95 6.0 ms. That Engine revision observed
the completed query 52–98.5 ms after submission, subtracts a 17 ms accelerated-renderer allowance,
and treats the remaining 35–81.5 ms observation delay as GPU pressure. Its selected duty therefore
falls near 20 percent and accepted presentation stretches to p50 155.9 ms / p95 266.9 ms. Input to
the browser frame remains p95 2.7 ms and authoritative RTT remains p95 59.016 ms, isolating the
large cadence error from gameplay scheduling and transport.

Rusty Engine #6436 owns the reusable correction: a valid timer query on positively identified
accelerated hardware must pace from measured GPU work without classifying delayed result polling
as GPU execution, while the completion fence still prevents a second in-flight submission.
Software-renderer backpressure, latest-wins demand, one RAF owner, animation/particles, explicit
render/reset, picking, statistics, and lifecycle remain required. VC9 will consume the reviewed
public Engine descendant and rerun this exact profile plus the unchanged full browser campaign
before the red rows can close.

The first public provider checkpoint,
`43866fa2172e940a5845a5d4be78db071c048cd8`, is measured at exact Demo revision
`5ae36dd5e41c0b64c848cacbcd4bc985f1640db1`. It correctly makes the valid accelerated timer query
authoritative: timer and effective-duration p95 are both 6.814 ms instead of including query
observation delay. The retained cadence is still red at p50 50.1 ms, p95 83.4 ms, and p99
166.9 ms. Completion observation remains p50 51.7 ms, and the latest sample observes admission
40.86 ms after its deadline. Input-to-next-frame p95 is 3.6 ms and synchronous submission p95 is
2.6 ms. This isolates a second Engine-owned issue: the host does not poll and admit ready
accelerated work promptly enough to use the corrected duration. The machine-readable report now
records this checkpoint; #6436 remains open for the readiness-scheduling descendant.

The next provider checkpoint,
`4dfbaf771511058a29da3d85134353e7b0e84a1d`, adds a bounded accelerated-readiness poll and is
measured at exact Demo revision `78359ea974bc79030cf446a1d044e676b5bb6216`. It is also red:
accepted cadence remains p50 50.1 ms, p95 83.4 ms, and p99 83.5 ms despite timer/effective p95
6.858 ms. Completion observation is p50 53.1 ms, and the latest deadline is observed 46.96 ms
late. The samples include the new `ready` state, proving that readiness can be reached without a
prompt accepted submission on the real continuous-camera path. #6436 therefore retains the
bounded poll but continues at the provider demand/RAF ownership boundary; no downstream scheduler
or degraded scene is introduced.

The accelerated eight-slot fence/query provider checkpoint,
`5dd9ff6dc6b387739ee5134eea1382983c05c247`, is measured at exact Demo revision
`425371360250ffb4aaf9396686f77aadb64b047b`. It also remains red: accepted cadence is p50
50.0 ms and p95/p99 83.4 ms while timer and effective-duration p95 remain only 6.901 ms.
Synchronous backend submission p95 is 2.5 ms, and completion observation remains p50 50.2 ms.
The latest sample observes admission about 48.24 ms after its computed deadline. The public pacing
sample does not expose the configured ring capacity or pending fence/query occupancy, so the
downstream report cannot prove that the accelerated path actually admitted more than one command
stream. #6436 retains ownership of both the missing bounded-ring evidence and the continuous-camera
cadence defect; the Demo does not add its own frame scheduler or weaken the VC9 budgets.

The capacity-observable provider checkpoint,
`24339f37ad5734ea92392602ffc024ca2c7e2f13`, is measured at exact Demo revision
`1ea37cbe0200615cf5aa012f2fe59a706d99c4f3`. It selects an eight-slot timer-query ring and an
active eight-slot completion-fence ring. Across 51 product samples, however, pending timer
measurements and pending fences both remain exactly one at p50, p95, p99, and maximum. Accepted
cadence remains p50 50.0 ms and p95/p99 83.3 ms even though timer/effective p95 is 6.724 ms and
synchronous backend p95 is 2.5 ms. This moves the remaining defect ahead of backend capacity into
the renderer-host/backend admission boundary. The exact report now distinguishes selected capacity
from observed occupancy so a nominal ring is not mistaken for exercised concurrency.

The host-admission-observable checkpoint,
`266f60c93531631a6ce0cb0aff26d966e95a3903`, is measured at exact Demo revision
`67e020072385ae6749f85205cfeabf5278ada496`. Its immutable 64-attempt ledger rules out both absent
continuous demand and sparse browser callbacks. Across 208 deduplicated attempts, every attempt
had retained-animation demand and `shouldSubmit=true`; 180 were admitted, 28 were backend-blocked,
and none had no demand. Lifetime totals at the final sample were 420 attempts, 380 admissions, 40
backend blocks, and zero no-demand decisions. Recent RAF intervals were p50 16.7 ms and p95
66.8 ms. The backend selected limit 8 throughout, but pre-attempt timer-query and fence occupancy
both remained p95/max 1/1. The latest rejected attempt carried requested, presentation, and
retained-animation demand while the backend reported `waiting`, query occupancy 1, and fence
occupancy 1. Product cadence remains p50 50.1 ms and p95 83.4 ms while timer/effective work is
p95 6.878 ms. The host therefore reaches the backend with real continuous demand, but backend
readiness rejects work while only one of eight slots is occupied; #6436 retains that owning fix.

The accepted phase-attribution provider,
`0e0c49442d0c3d876a1336a5a829087f6e2314db`, is measured at exact Demo revision
`db25b6b95a0d1bc3d39ae8aca2aa4fecae898123`. It proved that the remaining slowdown was in
Demo-owned observation rather than Engine admission: every recent attempt is admitted, RAF and
accepted-submission p95 are both 16.8 ms, callback-entry delay p95 is 1.3 ms, complete callback
p95 is 3.7 ms, and backend submission p95 is 2.5 ms. The Demo now transfers the immutable
64-attempt admission ledger once at the end of a profile instead of serializing it into the DOM on
every frame. Cosmetic viewmodel bob and impulse updates also stop cloning the complete retained
projection; canonical projection inspection still runs on first publication, reset/disposal, and
every weapon, visibility, mount, or node-count transition.

The resulting hardware cadence is p50 16.7 ms and p95/p99 16.8 ms, input-to-next-frame p95/max is
3.5/4.2 ms, and timer-query work remains p95 6.62 ms. The historical command-acknowledgement target
is still explicitly red at p95 52.958 ms, although its 55.598 ms maximum is within the 100 ms
ceiling. This row is not reclassified or relaxed; the normal-control retained-transport campaign
keeps its separate `< 2,000 ms` worst-case gate.

Studio evidence remains within its explicit bounds: all 342 route placements publish in bounded
32-entry batches; the structural readout is below 2 MiB; the 12-instance animated preview adds
exactly two geometry, two material, two texture, and twelve animated resources; close reaches zero
canvases; resize, reset, remount, reload, and fresh-process readback retain their canonical hashes.

## Camera policy

The original hardware-backed LAN baseline measured command acknowledgement p95 at 54.3 ms. The
complete-campaign certification reduces that to 45.28 ms, but the browser retains its bounded local
look offset so mouse presentation does not wait even one accepted tick. It presents only the
admitted portion of at most one in-flight and one coalesced pending look frame. Each axis is bounded
to two normalized input units. Accepted authoritative snapshots remain the base pose;
acknowledgement or rejection removes the corresponding frame, and restart/reconnect, blur,
pointer-lock loss, route disposal, and explicit input clearing discard the whole offset.

The offset is presentation-only. Rust still owns aim, fire, facts, persistence, collision, world
mutation, and the accepted player pose.

## 2026-07-31 installed Tauri baseline

`pnpm run certify:tauri-deploy` is the installed-product counterpart to the #6292 browser profile
and #6359 Radeon renderer profile. It does not relabel either predecessor. The first local run used
the independently approved DT1 source `cb080131810d1ed338379c5edd044df7b99a6e18`, Engine
`0e0c49442d0c3d876a1336a5a829087f6e2314db`, and Debian SHA-256
`ef75c7081b204251b6d084b144fc4106c719564dcec3e52a9514e2b631b9bae5` as the immutable input while
developing the DT2 installer and certification path.

| Installed measurement               | Exact result                                  |
| ----------------------------------- | --------------------------------------------- |
| Debian / AppImage bytes             | 9,762,212 / 86,526,456                        |
| Installed release-tree bytes        | 37,866,191                                    |
| WebKit cold menu                    | 697.646 ms                                    |
| Separate-process menu relaunch      | 1,507.180 ms                                  |
| New Game to first retained frame    | 1,625.112 ms                                  |
| Native process-tree RSS in gameplay | 1,111,855,104 bytes                           |
| WebView                             | WebKitGTK 60.5 compatibility user agent       |
| Xvfb WebGL identity                 | WebGL 2.0 / Apple GPU compatibility renderer  |
| Installed-sidecar campaign RTT max  | 103.5 ms                                      |
| Installed-sidecar queue/drop maxima | input/edge/outbound 1/1/1; dropped facts zero |

The native process tree includes the Tauri shell, Rust sidecar, WebKit network and Web processes,
and WebKit's sandbox helpers. It is the directly comparable packaged footprint missing from #6292;
it is not lower-level engine allocation. The WebDriver run uses Xvfb/WebKit and therefore cannot
replace #6359's Radeon 780M / Chromium cadence and 45.28 ms interaction measurement. Conversely,
the #6292 166.090/70.248 ms Chromium cold/warm navigation numbers cannot be called native launch
times. Keeping those environments separate prevents a synthetic comparison from becoming an
invented regression or hardware claim.

The native proof additionally records the 960×540 supported minimum without horizontal overflow,
singleton focus delegation without a second sidecar, renderer disposal/remount, real WebKit and
the expected absence of a WebGL context, idle CPU/context-switch deltas, normal shutdown, visible startup/host-crash errors,
shell-crash orphan cleanup, and full/narrow screenshots. Certification also runs the browser
HUD/control shell against the installed sidecar and installed Web assets, reads authoritative Rust
state, and proves that the browser owns neither a canvas nor renderer/input authority. Native input,
picking, resize, reset, remount, resource rendering, save round-trip, and disposal remain certified
by the Engine-owned native host. The exact-revision `verify-tauri` artifact
`tauri-deployment-evidence.json` is the final release evidence; a focused run with
`--skip-campaign` is explicitly non-certifying.
