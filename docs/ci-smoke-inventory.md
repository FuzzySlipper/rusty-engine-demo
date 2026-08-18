# CI and smoke inventory

The default gate answers whether a change keeps the authored content, the gameplay semantics, and
the product shells coherent. Boundary auditing, native-host proof, release packaging, and long
playthrough certification are separate on-demand operations, not per-commit CI.

Rusty Crew reads [`.rusty-crew-review.json`](../.rusty-crew-review.json) and gates managed reviews
on the automatic `verify` check only. Tauri verification is intentionally relevance-triggered.

## Default gate (`pnpm run verify`, task 7052 lean chain)

| Check                       | Why it remains                                                                                            |
| --------------------------- | --------------------------------------------------------------------------------------------------------- |
| `pnpm run typecheck`        | TypeScript across the shell, libs, and TS packages, including the gameplay authoring workspace.           |
| `pnpm run gameplay:check`   | Materializes the authored gameplay package and fails on drift against the committed canonical artifact.   |
| `pnpm run check:content`    | Regenerates the canonical E1M1 project byte-identically (project admission + canonicalization).           |
| `pnpm run test:ts`          | Project-content unit tests.                                                                               |
| `pnpm run build:shell`      | The browser shell builds.                                                                                 |
| `./scripts/verify-rust.sh`  | `cargo fmt --check`, the gameplay-package parity bin, focused Rust suites, and `clippy -D warnings`.       |

GitHub Actions currently runs a slightly wider explicit step list (boundary audit plus the
platform/shell/engine-route TS suites); task 7053 collapses the workflow onto this lean chain.

## On-demand commands

| Command or workflow            | When to use it                                                                                                  |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| `pnpm run verify:native`       | Renderer/host work: native-host proof and the E1M1 native playthrough with screenshot capture.                  |
| `pnpm run audit:boundary`      | Boundary-contract changes: downstream TS/Rust surfaces, Engine dependency shape, forbidden vocabulary.          |
| `pnpm run test:platform` / `test:shell` / `test:engine-route` | Shell or platform changes touching those packages.                                 |
| `pnpm run smoke:e1m1`          | Starts the Doom host, selects E1M1 from the real menu, mounts one Engine canvas, checks rendered pixels.        |
| `pnpm run certify:e1m1`        | Before claiming the complete E1M1 switch, combat, traversal, terrain, and exit route works.                     |
| `verify Tauri` GitHub workflow | For desktop, sidecar, package-layout, or Tauri process-lifecycle changes. Runs `pnpm run verify:tauri`.         |

## Retired checks

- `scripts/browser-smoke.mjs` combined unrelated migration, Studio, asset, routing, input, save, and
  lifecycle scenarios in a 3,209-line campaign. Focused shell tests, native verification, the E1M1
  renderer smoke, and the desktop smoke own those distinct claims.
- `headless-door`, `headless-encounter`, and `headless-beacon` only ran already-covered game paths
  and discarded their output. Focused Rust integration tests retain those behavioral assertions.
- Routine Tauri CI no longer builds and installs release bundles after already verifying the direct
  desktop binary. Package installation and artifact upload belong to the manual release workflow.
- Installed-product certification no longer launches a second browser-control campaign. The
  installed Tauri/WebDriver smoke owns that product claim.
- Task 7052 removed the exhaustive Rust ceremony suites (studio adapter, project codec/store
  round-trips, voxel golden files, per-room Doom suites — 26 integration suites) and the
  boundary-audit test harness (`audit-active-guidance.test.mjs`, `test-boundaries.mjs`). The
  retained Rust suites cover combat resolution (player and enemy), pickups/inventory,
  door/interaction, vitality/death, save/restart, the fixed-tick product loop over canonical
  projects, and the upstream adoption seams.

Add a smoke only when it owns a product boundary that cannot be asserted more cheaply, has a clear
failure signal, and is not already covered by a focused test or another retained smoke.
