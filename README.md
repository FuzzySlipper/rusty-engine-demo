# Rusty Engine Demo

Rusty Engine Demo is the first external game built on
[Rusty Engine](https://github.com/FuzzySlipper/rusty-engine). It owns the loading-bay game's
components, services, project schema, scheduling, persistence, authored content, browser product,
and user-facing acceptance. Rusty Engine owns reusable entity, spatial, collision, navigation,
voxel, mesh, asset, and conversion mechanisms.

The Rust game vertical is named `loading-bay-game`. Its Rust and browser Engine dependencies are
public Git dependencies pinned to reviewed provider revision
`9813bf6f759a8967a5de1681d4726f7b17254ca5`; a sibling checkout is not required. The demo owns its
Angular/Nx browser shell, route-scoped input lifecycle, and semantic projection adapter, while Rusty
Engine owns the shared render contracts, retained projection, Three/WebGL backend, surface host,
audio, particle, billboard, and telemetry hosts.

The demo-owned `ExtractionBeacon` is the first post-extraction gameplay addition. Its authored
configuration and live state remain on the beacon entity, `ExtractionBeaconService` owns the
activation rules, and accepted activation produces a typed fact consumed by bounded browser
presentation. It deliberately adds no Engine vocabulary.

## Run and verify

Install the pinned JavaScript toolchain, build the shell, and launch the Rust host:

```bash
pnpm install --frozen-lockfile
pnpm run build:shell
cargo run --locked -p loading-bay-game --bin browser-host
```

Then open `http://127.0.0.1:8787`. The default project is
`content/projects/loading-bay.project.json`; pass `--project <path>` to select another admitted
project. `content/projects/relay-annex.project.json` is a second serialized arrangement using the
same settled demo meanings. Both files are canonical Studio-owned project
artifacts; ordinary fixture generation never rewrites them.

The root route is a full-viewport FPS surface with a disposable HUD projection. Its diagnostics
drawer exposes only concrete Rust host actions. Renderer-owned frame cadence and synchronous
backend submission time are shown separately from server tick, snapshot cadence, payload, input,
and command-RTT counters; backend submission is not GPU timing. The hash-routed diagnostics screen
is also the browser lifecycle proof: leaving the game route releases the shared renderer before
another route can mount it.

For a managed LAN-facing session, use the repository manifest:

```bash
den-serve up rusty-engine-demo -repo /absolute/path/to/rusty-engine-demo
```

The launcher builds the browser shell, starts the Rust host on the broker-selected LAN bind and
port, and reports the local and LAN URLs. Set `RUSTY_ENGINE_DEMO_PROJECT` to another project path
before `den-serve up` when needed. The process group remains owned by `den-serve`; use its `status`,
`logs`, and `stop` commands with the same project and repository arguments.

The complete product gate is:

```bash
pnpm run verify
```

It checks package and repository boundaries, TypeScript content and presentation tests, the exact
Engine Git resolution, the complete Rust suite and Clippy, and a real Chromium/Three/WebGL flow.
For Rust-only iteration, `./scripts/verify-rust.sh` remains available.

For focused diagnosis:

```bash
pnpm run check:content       # generated fixtures match; canonical projects Rust-admit and round-trip
pnpm run test:shell          # protocol, projection, input, and presentation units
pnpm run test:browser        # real Chromium campaign and lifecycle proof
pnpm run audit:boundary      # exact pins and forbidden downstream shortcuts
```

`GET /health` identifies a running browser host and `GET /api/state` is a read-only diagnostic
snapshot. Live gameplay commands use the bounded `/api/session` WebSocket; do not debug by adding
HTTP gameplay mutators. `pnpm run generate:content` updates only deliberate fixtures under
`content/generated`; use the Studio/adapter save path for canonical project edits.

The project-owned Studio adapter can be run as a bounded JSON-lines process:

```bash
cargo run --locked -p loading-bay-game --bin studio-adapter
```

It opens only an explicit absolute project root and safe relative project file supplied through the
closed protocol. It exposes Engine-owned catalog, scene, entity, persistence, voxel inspection, and
renderer projection readouts while Loading Bay retains its schema and domain-operation meaning. See
[docs/studio-adapter.md](docs/studio-adapter.md).

## Architecture boundary

- Rust owns live gameplay state, substantial logic, explicit scheduling, typed consequences, and
  persistence.
- TypeScript may compose deliberate migration fixtures and host input/presentation; canonical game
  scenes remain serialized Studio-owned projects and TypeScript does not become a second gameplay
  runtime.
- Game-specific presentation code maps typed Rust facts into shared render descriptors. It does not
  own a second renderer, resource cache, effect simulation, or cleanup runtime.
- Dependency direction is `rusty-engine-demo -> rusty-engine` only.
- New game semantics belong here first. Do not add a generic Engine hook merely to make a demo
  feature fit.

The campaign contract for this compact, original FPS is
[docs/fps-product-architecture.md](docs/fps-product-architecture.md). It records the current
proof-shaped baseline, sole-authority map, fixed Rust phase order, bounded game-session contract,
cold/dynamic/transient state split, original level route, measurable product budgets, and
acceptance corpus. The authored route and its browser playtest checkpoints are recorded in
[docs/loading-bay-playtest.md](docs/loading-bay-playtest.md).

Concrete recipes for extending items, weapons, enemies, progression objects, and level content
without creating parallel authority are in
[docs/extension-recipes.md](docs/extension-recipes.md). The implemented wire lifecycle and bounds
are in [docs/game-session-protocol.md](docs/game-session-protocol.md). Active limitations are owned
by the Den document `rusty-engine-demo/known-limitations`; update it with an owning task when a
limitation changes.

Exact transfer provenance is recorded in [docs/source-provenance.md](docs/source-provenance.md).
Renderer/session counter semantics, budgets, and the current headed LAN baseline are recorded in
[docs/performance.md](docs/performance.md).
The Studio-authored visual-content campaign's authority map, serialized prop/weapon kit, CC0
source and license hashes, voxel-brush experiment, and exact comparison baselines are in
[docs/visual-content-pipeline.md](docs/visual-content-pipeline.md).
The imported industrial prop sources are Kenney CC0 1.0 assets; their copied notices and exact
source/derivative hashes are recorded in
[docs/source-provenance.md](docs/source-provenance.md#vc5-serialized-industrial-prop-kit).
The production actor kit is likewise reproducible from Kenney's CC0 Animated Characters Retro
pack. Its Blender 5.1.2 recipe, exact six-clip GLB hashes, embedded skins, copied notice, Rust-owned
Studio import, and shared-renderer lifecycle proof are recorded in
[docs/source-provenance.md](docs/source-provenance.md#vc4-production-animated-actor-source-kit).
The canonical schema-23 project binds those actors and the industrial prop kit to Rust-owned
gameplay posture/state through versioned renderer-only visual bindings; the ownership and
full-product proof are documented in
[docs/visual-content-pipeline.md](docs/visual-content-pipeline.md#vc8-serialized-gameplay-visual-bindings).

The repository's one active Rusty Engine revision is declared in
[`engine-source.json`](engine-source.json). Check it with `./scripts/engine-revision check`, preview
an update without mutating the checkout with
`./scripts/engine-revision update <40-character-public-sha> --dry-run`, or apply the validated
carrier-only update with `./scripts/engine-revision update <40-character-public-sha>`. The updater
does not commit, push, rewrite historical evidence, or change protocol fixtures. Its complete
preflight, candidate, failure, and recovery contract is documented in
[docs/engine-revision-updates.md](docs/engine-revision-updates.md).
