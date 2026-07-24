# Rusty Engine Demo

Rusty Engine Demo is the first external game built on
[Rusty Engine](https://github.com/FuzzySlipper/rusty-engine). It owns the loading-bay game's
components, services, project schema, scheduling, persistence, authored content, browser product,
and user-facing acceptance. Rusty Engine owns reusable entity, spatial, collision, navigation,
voxel, mesh, asset, and conversion mechanisms.

The Rust game vertical is named `loading-bay-game`. Its Engine dependencies are public Git
dependencies pinned to exact revision `a2e55f9660e46751d4c78bcdd23b9a321b0dc961`; a sibling checkout
is not required. That revision is the verified M9 handoff and will be re-pinned if its active review
produces a successor commit.

## Current extraction phase

M10A ports the complete Rust gameplay vertical and its focused/headless tests. The TypeScript
content-composition and browser/Three product move in the dependency-ordered M10B slice; until then,
the copied `browser-host` binary compiles and its Rust routes are tested, but a browser distribution
is intentionally not present in this repository.

## Verify the Rust vertical

```bash
./scripts/verify-rust.sh
```

This checks formatting, exact Git dependency resolution, the complete Rust test suite, Clippy, and
the headless door/encounter paths.

## Architecture boundary

- Rust owns live gameplay state, substantial logic, explicit scheduling, typed consequences, and
  persistence.
- TypeScript may compose immutable project content and host input/presentation; it does not become a
  second gameplay runtime.
- Dependency direction is `rusty-engine-demo -> rusty-engine` only.
- New game semantics belong here first. Do not add a generic Engine hook merely to make a demo
  feature fit.

Exact transfer provenance is recorded in [docs/source-provenance.md](docs/source-provenance.md).
