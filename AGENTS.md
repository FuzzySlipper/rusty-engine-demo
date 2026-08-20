# Rusty Engine Demo agent guidance

Loading Bay is the external game and reference consumer of Rusty Engine. It is one product, not an Asha compatibility layer or a place to generalize speculative Engine APIs. Doom E1M1 is its sole supported authored content.

## Den guidance

- Project ID: `rusty-engine-demo`.
- Resolve live guidance with Den `get_agent_guidance` before substantial work.
- Treat that packet and its referenced documents as current task and review policy.
- If Den is unreachable, stop and report the failed tool; do not reconstruct Den state locally.

## Ownership and boundaries

- Rust owns project admission, live gameplay state, validation, mutation, scheduling, facts, saves, runtime snapshots, and product projection. `LoadingBayProductService` is the named, transport-neutral product authority.
- TypeScript composes immutable E1M1 content, captures semantic input, and owns disposable HUD and product-shell presentation. It neither evaluates gameplay nor imports/configures the Engine renderer.
- `browser-host` is the lightweight HTTP/WebSocket development adapter. Tauri is one product WebView using typed in-process IPC over the same service; it has no browser-host sidecar.
- Rusty Engine alone owns rendering and the canvas. Use its public application-host surface and its Rust webview adapter; never reach a private bridge or packaged renderer artifact.
- Keep one unconditional Cargo path dependency on the complete adjacent `rusty-engine` facade. Preserve explicit `rusty_engine::<owner>` namespaces. Do not pin, manage, or select Engine subcrates downstream.
- Add game-specific semantics here. Promote a smaller Engine seam only after another real consumer proves it reusable. Do not add a plugin registry, service locator, generic bridge, behavior IR, replay spine, or TypeScript gameplay authority.
- Keep authored content, live runtime state, and transient presentation distinct. Preserve exact E1M1 source and asset provenance in `docs/source-provenance.md`.

## Proof

- Run `./scripts/verify-rust.sh` for Rust-only work and `pnpm run verify` for product-visible or cross-language work.
- Run focused browser smoke for browser-relevant changes. Run Tauri contract/build checks for Tauri-relevant changes; headed WebView evidence is required before claiming a visible or packaged desktop result.
- `pnpm run certify:e1m1` is a release/manual route, not a default gate. It currently stalls at waypoint `[127,121]`; do not represent it as passing.
