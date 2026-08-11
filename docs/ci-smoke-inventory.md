# CI and smoke inventory

The default gate should answer whether a change preserves repository boundaries, authored content,
gameplay authority, the Engine renderer integration, and the desktop host. Release packaging and
long playthrough certification are separate operations, not per-commit CI.

## Default push and pull-request gates

| Check                                                             | Why it remains                                                                                                                                                                                                           |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Boundary, TypeScript, content, shell, and performance-tool checks | Fast focused checks with distinct ownership and failure identity.                                                                                                                                                        |
| `./scripts/verify-rust.sh`                                        | Formats, tests, and lints the downstream gameplay crate against the adjacent Engine facade.                                                                                                                              |
| `./scripts/verify-native-host.sh`                                 | Proves the game-owned native host can mount the Engine Rust adapter and apply game consequences.                                                                                                                         |
| `pnpm run smoke:e1m1`                                             | Starts the Doom host, selects E1M1 from the real menu, mounts one Engine canvas, checks complete Rust-authored render content, and rejects blank output by sampling rendered pixels. It stops before gameplay traversal. |
| `pnpm run verify:tauri`                                           | Proves the directly built desktop shell, sidecar, package layout, and process lifecycle once.                                                                                                                            |

## Explicit certification

| Command or workflow                     | When to use it                                                                                                  |
| --------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `pnpm run certify:e1m1`                 | Before claiming the complete E1M1 switch, combat, traversal, terrain, and exit route works.                     |
| `certify Tauri release` GitHub workflow | When producing a Linux release artifact. It builds, installs, certifies, fingerprints, and uploads the package. |
| Performance and capture commands        | When changing performance-sensitive code or refreshing named visual evidence.                                   |

## Retired checks

- `scripts/browser-smoke.mjs` combined unrelated migration, Studio, asset, routing, input, save, and
  lifecycle scenarios in a 3,209-line campaign. Its only active callers selected a reduced control
  branch. Focused shell tests, native verification, the E1M1 renderer smoke, and the desktop smoke
  now own those distinct claims.
- `headless-door`, `headless-encounter`, and `headless-beacon` only ran already-covered game paths
  and discarded their output. Focused Rust integration tests retain those behavioral assertions.
- Routine Tauri CI no longer builds and installs release bundles after already verifying the direct
  desktop binary. Package installation and artifact upload belong to the manual release workflow.
- Installed-product certification no longer launches a second browser-control campaign. The
  installed Tauri/WebDriver smoke owns that product claim.

Add a smoke only when it owns a product boundary that cannot be asserted more cheaply, has a clear
failure signal, and is not already covered by a focused test or another retained smoke.
