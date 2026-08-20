# Loading Bay desktop shell

Tauri packages Loading Bay as one final-product WebView over the same Angular build used by the browser shell. It is an in-process adapter over `LoadingBayProductService`.

## Contract

At startup Tauri loads the packaged E1M1 content closure, creates the product service, and exposes typed commands for service readout, session start/disconnect, command submission, and projected application resources. Rust owns fixed-step ticking for the active desktop session; the WebView polls readout and never advances gameplay. The WebView selects that transport in desktop mode; browser mode continues to use `loading-bay.v2` through `browser-host`.

There is no `browser-host` sidecar, loopback port, readiness polling, asset-hash handshake, orphan process cleanup, or second product window. Engine remains the only canvas/renderer owner.

## Verification

```bash
pnpm run test:tauri      # typed source/contract coverage
pnpm run build:tauri:binary
pnpm run smoke:tauri     # contract smoke
pnpm run smoke:tauri:headed # headed WebView evidence: menu, E1M1 frame, typed session, shutdown
pnpm run verify:tauri
```

These are relevance-triggered checks, not the default CI gate. A source/contract check does not prove a visible or packaged desktop experience. Before making either claim, obtain a headed WebView run that observes one rendered frame, a typed session round-trip, packaged resources, and clean shutdown.
