# Rusty Engine Demo

Rusty Engine Demo is the first external game built on
[Rusty Engine](https://github.com/FuzzySlipper/rusty-engine). It owns the loading-bay game's
components, services, project schema, scheduling, persistence, authored content, browser product,
and user-facing acceptance. Rusty Engine owns reusable entity, spatial, collision, navigation,
voxel, mesh, asset, and conversion mechanisms.

The Rust game vertical is named `loading-bay-game`. Its Engine dependencies are public Git
dependencies: Rust remains pinned to reviewed gameplay revision
`e462016d5cafe87ce1994d33f7791f41fa9bd727`, while the browser renderer is pinned to review-fix
revision `937a3cef2568d04a261e78126f34e6baea1828c9`; a sibling checkout is not required. The demo owns its browser shell and semantic projection adapter,
while Rusty Engine owns the shared render contracts, retained projection, Three/WebGL backend,
surface host, audio, particle, billboard, and telemetry hosts.

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
project. `content/projects/relay-annex.project.json` is a second, entirely TypeScript-authored
arrangement using the same settled demo meanings.

The complete product gate is:

```bash
pnpm run verify
```

It checks package and repository boundaries, TypeScript content and presentation tests, the exact
Engine Git resolution, the complete Rust suite and Clippy, and a real Chromium/Three/WebGL flow.
For Rust-only iteration, `./scripts/verify-rust.sh` remains available.

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
- TypeScript may compose immutable project content and host input/presentation; it does not become a
  second gameplay runtime.
- Game-specific presentation code maps typed Rust facts into shared render descriptors. It does not
  own a second renderer, resource cache, effect simulation, or cleanup runtime.
- Dependency direction is `rusty-engine-demo -> rusty-engine` only.
- New game semantics belong here first. Do not add a generic Engine hook merely to make a demo
  feature fit.

Exact transfer provenance is recorded in [docs/source-provenance.md](docs/source-provenance.md).
