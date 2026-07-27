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
