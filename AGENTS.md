# Loading Bay agent guidance

Loading Bay is one C# NativeAOT reference product for Rusty Engine. Doom E1M1 is its sole supported authored content. It is not an Asha compatibility layer, a fixture gallery, or a place to generalize speculative Engine APIs.

## Den

- Project: `rusty-engine-demo`.
- Before substantial work, resolve live guidance with Den `get_agent_guidance` and follow its referenced task/review policy.
- If Den is unavailable, report the failed operation and stop; do not reconstruct task state locally.

## Ownership

- `csharp/LoadingBay.Game` owns product meaning: E1M1 policy, validation, game state, save meaning, facts, snapshots, and HUD projection. Keep values typed and named in `LoadingBayTuning`, definitions, and product records; do not hide gameplay policy in transport or UI code.
- `csharp/LoadingBay.NativeProduct` is the small NativeAOT composition root. Keep the one complete adjacent `rusty-engine` facade dependency; use its public C# contracts and explicit owner namespaces. Do not select Engine subcrates, P/Invoke private bridges, or manage the Engine checkout downstream.
- Rusty Engine owns application lifecycle, admitted timing, input transport, renderer/canvas, spatial/voxel mechanisms, persistence primitives, content services, and host integration. Promote a neutral Engine seam when this product needs an Engine-owned mechanism; do not duplicate it here.
- TypeScript is the Angular/DOM shell: semantic input capture, renderer preload, and disposable read-only HUD presentation. It must not evaluate gameplay, construct live game state, configure a renderer, or own a second loop/canvas.
- The public Product Browser Host is the browser development adapter. There is no retained WebSocket product protocol, browser-host sidecar, Tauri contract, compatibility bridge, plugin registry, service locator, behavior IR, replay spine, or TypeScript gameplay authority.
- Keep authored E1M1 content, live C# runtime state, and transient presentation separate. Preserve the exact source and asset provenance in `docs/source-provenance.md`.

## Proof

- C# changes: `./scripts/verify-csharp-spine.sh` (managed build, lifecycle exercise, NativeAOT publish).
- Browser-facing changes: build the Angular shell, run `./scripts/run-csharp-product.sh`, and obtain focused browser evidence of the one Engine canvas plus the HUD/runtime continuation actually affected.
- Content/provenance changes: run the relevant deterministic content/provenance check; do not make the C# runtime parse source-shaped authoring data.
- `pnpm run certify:e1m1` is release/manual only and currently stalls at waypoint `[127,121]`. It is not passing certification or proof of complete E1M1 traversal.
