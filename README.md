# Rusty Engine Demo

Rusty Engine Demo is the first external game built on
[Rusty Engine](https://github.com/FuzzySlipper/rusty-engine). It owns the loading-bay game's
components, services, project schema, scheduling, persistence, authored content, browser product,
and user-facing acceptance. Rusty Engine owns reusable entity, spatial, collision, navigation,
voxel, mesh, asset, and conversion mechanisms.

The Rust game vertical is named `loading-bay-game`. It consumes one rolling-current public
`rusty-engine` facade dependency, with the exact resolved SHA recorded in the lock and
`engine-source.json`; a sibling checkout is not required. The demo owns its Angular/Nx browser
control/HUD shell and the native product window, timing, semantic input mapping, resource policy,
and gameplay consequences. Rusty Engine owns retained rendering, the private TypeScript boundary
and artifact, and renderer lifecycle behind a named Rust host adapter.

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

The production native shell uses the same Angular build and Rust host:

```bash
pnpm run verify:tauri       # native binary, real WebKit smoke, and process lifecycle
pnpm run build:tauri        # deb + AppImage on the documented Ubuntu baseline
```

The directly rendered Rust-native product and its real X11 proof are:

```bash
pnpm run native
pnpm run native:doom      # interactive E1M1; WASD moves, arrow keys look
pnpm run verify:native
```

The directly runnable binary is `target/release/loading-bay-desktop`. Its sidecar and resource
tree are intentionally separate build outputs and must stay beside it in the layout documented in
[docs/tauri-desktop.md](docs/tauri-desktop.md). The installable Linux bundles carry that layout
themselves.

An exact reviewed Debian artifact can be installed without root using `pnpm run deploy:tauri --
install ...`; status, atomic rollback, ordinary uninstall with save preservation, explicit data
purge, desktop entry, and the installed-product certification command are documented in
[docs/tauri-desktop.md](docs/tauri-desktop.md#local-deployment).

The root browser route, including sessions started by `den-serve`, composes the rich Angular HUD
and controls over one Engine-owned renderer through `@rusty-engine/application-host`. The browser
shell imports no renderer internals and owns no canvas, backend, frame decoder, or render loop.
Rust supplies the retained frame, camera, gameplay facts, and semantic input consequences; Engine
owns DOM/canvas composition, replacement, resize, pointer arbitration, and renderer lifecycle. The
same web bundle is used unchanged inside the Tauri wrapper.

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

It checks package and repository boundaries, TypeScript content and browser-shell tests, the
adjacent Engine facade and bundled application host, the complete Rust suite and Clippy, the
Engine-owned native adapter proof, and retained browser/Tauri product flows.
For Rust-only iteration, `./scripts/verify-rust.sh` remains available.

For focused diagnosis:

```bash
pnpm run check:content       # generated fixtures match; canonical projects Rust-admit and round-trip
pnpm run test:shell          # protocol, projection, input, and presentation units
pnpm run test:browser        # real Chromium campaign and lifecycle proof
pnpm run audit:boundary      # adjacent facades and forbidden downstream shortcuts
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
Native package layout, security boundaries, build baseline, and deployment commands are recorded
in [docs/tauri-desktop.md](docs/tauri-desktop.md).
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
The exact-time contact sheets, independent source-sampler comparison, and retained authored-motion
limitations are recorded in
[docs/animated-mesh-capture.md](docs/animated-mesh-capture.md).
The canonical schema-24 project binds those actors and the industrial prop kit to Rust-owned
gameplay posture/state through versioned renderer-only visual bindings; the ownership and
full-product proof are documented in
[docs/visual-content-pipeline.md](docs/visual-content-pipeline.md#vc8-serialized-gameplay-visual-bindings).
Renderable `localTransform` is a presentation-only offset composed after each entity's world
transform. Loading Bay uses it to place the Bay Rusher, Arc Warden, and differently pivoted
control-panel meshes on the contact plane without changing collision, navigation, combat, or
other world-space gameplay facts. Studio exposes and persists this offset separately from the
world transform.

The completed visual-content campaign is certified at Demo revision
`67e8d1d609f46d11fe8da0d990fc7a9b6ab33285`. It retains 89 serialized assets, 419 authored
entities, nine reusable voxel-brush definitions, 367 repeated brush instances, two animated actor
identities, and 33 capability-complete visual bindings without a downstream renderer, asset cache,
animation loop, or gameplay authority. The exact CI run and final desktop/narrow/Studio audit are
indexed by
[docs/evidence/final-visual-content-certification.json](docs/evidence/final-visual-content-certification.json).
With a healthy product host, reproduce the two game captures with:

```bash
RUSTY_ENGINE_DEMO_URL=http://127.0.0.1:8787/#/game?mode=new pnpm run capture:final
```

Cold-agent actor and voxel-brush import/place/save/reload recipes are in
[docs/extension-recipes.md](docs/extension-recipes.md#cold-agent-visual-content-reproduction).

Exact certification is declared in [`engine-source.json`](engine-source.json). Check it with
`./scripts/engine-revision certify check` (the legacy `check` alias remains available), preview
an exact update without mutating the checkout with
`./scripts/engine-revision update <40-character-public-sha> --dry-run`, or apply the validated
carrier-only update with `./scripts/engine-revision certify update <40-character-public-sha>`. For
rolling development, `./scripts/engine-revision dev sync --json` follows the current public Engine
line, reports the resolved SHA, and records operational state without a compatibility promise. The
updater does not commit, push, rewrite historical evidence, or change protocol fixtures. Its complete
preflight, candidate, failure, and recovery contract is documented in
[docs/engine-revision-updates.md](docs/engine-revision-updates.md).
