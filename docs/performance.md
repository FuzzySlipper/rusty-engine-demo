# Loading Bay performance evidence

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

There is no Tauri or other native desktop package in this repository yet. This is therefore a
desktop-representative WebView/browser measurement, not a native-shell launch, installer-size, or
process-memory certification. When a desktop shell exists, the same route and metrics must be run
inside it rather than treating these Chromium numbers as a substitute.

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

## 2026-07-30 content-rich static-instancing follow-up

Task #6354 now measures the unchanged serialized prop/viewmodel and 342-placement voxel-brush
product at exact Engine revision `6fe4713df76ce0a03a6c461dfa95d4a90b24c824`. The shared
renderer internally batches compatible opaque world static handles into deterministic 8-unit
world cells with conservative aggregate bounds and frustum culling while retaining every logical
handle, transform, metadata record, and picking identity. No content, downstream cache, render
loop, or gameplay authority was removed.

The predecessor `c903c1c86761386087acd7d7d814a3da5cde116b` reduced draw calls but formed
scene-wide batches with culling disabled. Exact CI run `30532095039` consequently expired after
Generator with a 7,506.8 ms maximum command RTT. That result is retained as the rejected
intermediate rather than being represented by the faster workstation sample. The first culling
revision, `e97944c8309018f595222edb7bd90a620c32cedf`, used 32-unit cells; this real project placed
367 instances into only three cells (222 / 126 / 19), and exact CI run `30535751200` still expired
after Generator at 9,152.7 ms maximum RTT. The reviewed 8-unit correction produces 29 cells with at
most 29 project instances per cell. The table below is its local headless-SwiftShader
correctness/resource run; exact-SHA CI remains the task review gate rather than a value inferred
from this table:

| Measurement                         | Result      |
| ----------------------------------- | ----------- |
| Session bootstrap                   | 1,159,686 B |
| Largest steady update               | 11,937 B    |
| Outbound / input / edge queue peaks | 1 / 1 / 1   |
| Dropped facts                       | 0           |
| Maximum authoritative command RTT   | 1,422.0 ms  |
| Renderer cadence                    | 16.6 ms     |
| Rich draw-call delta                | +32         |
| Rich live-handle delta              | +33         |
| Rich submitted-triangle delta       | +64         |

The ordinary product submission is 49 draws for 412 live handles and 94,193 submitted triangles.
The 32-instance stress probe reaches 81 draws, 445 handles, and 94,257 triangles, then cleanup
returns to 49 / 412 / 94,193. Geometry and material counts remain at 48 / 86 rather than the
44 / 82 pre-stress baseline because Engine #6416 deliberately retains four reusable static-mesh
definitions after their last live instances are destroyed. Reuse is bounded, and renderer disposal
remains the terminal release boundary.

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
