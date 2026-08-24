# Loading Bay

Loading Bay is a small Rust-authoritative game and reference consumer of [Rusty Engine](https://github.com/FuzzySlipper/rusty-engine). Its one supported authored experience is the Doom E1M1 Hangar recreation. “Loading Bay” remains the product, repository, crate, and protocol name; it is not a menu of legacy fixtures.

## Run

```bash
pnpm install --frozen-lockfile
pnpm run build:shell
cargo run --locked -p loading-bay-game --bin browser-host --release
```

Open `http://127.0.0.1:8787`. The browser host admits `content/projects/doom-e1m1.project.json` and is a development HTTP/WebSocket adapter over the Rust product service. Use the release host for ordinary play: unoptimized wgpu/wgpu-hal builds misrepresent frame pacing, so debug-host FPS numbers are not evidence.

## Clean clone

See [docs/downstream-onboarding.md](docs/downstream-onboarding.md) for the full prerequisites, generation/admission order, minimum copyable spine, and the ownership/code-routing table. In short: a sibling `../rusty-engine` checkout, Node ≥ 26 + pnpm ≥ 11 + stable Rust, then the three commands above. No WAD is needed to build, verify, or run — only to regenerate derived assets from sources (`pnpm run check:provenance`).

## Architecture

- `LoadingBayProductService` admits E1M1, owns the fixed-step runtime, semantic commands, saves, facts, and readouts. HTTP/WebSocket and Tauri IPC are adapters, not gameplay owners.
- The Angular shell uses the public `@rusty-engine/application-host`. Engine alone owns the renderer and canvas; the shell owns semantic input and disposable HUD presentation.
- Tauri packages the same shell as one WebView with typed in-process IPC. It does not launch a browser-host sidecar.
- This repo depends on the complete adjacent `rusty-engine` facade through one Cargo path dependency. Game semantics stay downstream.

## Verify

```bash
./scripts/verify-rust.sh  # Rust authority only
pnpm run verify           # default product gate
```

Run focused checks when their surface changes:

```bash
pnpm run test:shell
pnpm run test:engine-route
pnpm run test:platform
pnpm run audit:boundary
pnpm run smoke:e1m1       # browser-visible E1M1 smoke
pnpm run verify:tauri     # Tauri contract/build/smoke
```

The full `pnpm run certify:e1m1` route is release/manual work and currently stalls at waypoint `[127,121]`; it is not a passing certification claim.

## Documentation

- [design.md](docs/design.md) — authority and adapter boundary.
- [game-session-protocol.md](docs/game-session-protocol.md) — browser `loading-bay.v2` session.
- [tauri-desktop.md](docs/tauri-desktop.md) — in-process desktop contract and proof limits.
- [doom-e1m1-gameplay-ledger.md](docs/doom-e1m1-gameplay-ledger.md) — content calibration.
- [source-provenance.md](docs/source-provenance.md) — E1M1, WAD, sprite/texture, and prop closure.
- [extension-recipes.md](docs/extension-recipes.md) — safe downstream changes.
- [downstream-onboarding.md](docs/downstream-onboarding.md) — clean clone, minimum spine, ownership/code-routing table.
- [presentation-frame.md](docs/presentation-frame.md) — bounded-viewport presentation contract.
