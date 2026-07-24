# Rusty Engine Demo

Rusty Engine Demo is the first external game built on
[Rusty Engine](https://github.com/FuzzySlipper/rusty-engine). It owns the loading-bay game's
components, services, project schema, scheduling, persistence, authored content, browser product,
and user-facing acceptance. Rusty Engine owns reusable entity, spatial, collision, navigation,
voxel, mesh, asset, and conversion mechanisms.

The Rust game vertical is named `loading-bay-game`. Its Engine dependencies are public Git
dependencies pinned to exact revision `a2e55f9660e46751d4c78bcdd23b9a321b0dc961`; a sibling checkout
is not required. The browser shell and renderer are demo-owned packages under the
`@rusty-engine-demo` scope.

## Run and verify

Install the pinned JavaScript toolchain, build the shell, and launch the Rust host:

```bash
pnpm install --frozen-lockfile
pnpm run build:shell
cargo run --locked -p loading-bay-game --bin browser-host
```

Then open `http://127.0.0.1:8787`. The default project is
`content/projects/loading-bay.project.json`; pass `--project <path>` to select another admitted
project.

The complete product gate is:

```bash
pnpm run verify
```

It checks package and repository boundaries, TypeScript content and presentation tests, the exact
Engine Git resolution, the complete Rust suite and Clippy, and a real Chromium/Three/WebGL flow.
For Rust-only iteration, `./scripts/verify-rust.sh` remains available.

## Architecture boundary

- Rust owns live gameplay state, substantial logic, explicit scheduling, typed consequences, and
  persistence.
- TypeScript may compose immutable project content and host input/presentation; it does not become a
  second gameplay runtime.
- Dependency direction is `rusty-engine-demo -> rusty-engine` only.
- New game semantics belong here first. Do not add a generic Engine hook merely to make a demo
  feature fit.

Exact transfer provenance is recorded in [docs/source-provenance.md](docs/source-provenance.md).
