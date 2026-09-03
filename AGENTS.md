# Loading Bay agent guidance

Loading Bay is an ordinary C# product using the packaged Rusty Engine SDK. Doom E1M1 is its sole supported authored content. CoreCLR through `rusty dev` is the normal development and Den lane; NativeAOT is an explicit fidelity/release check.

## Den

- Project: `rusty-engine-demo`.
- Before substantial work, resolve live guidance with Den `get_agent_guidance` and follow its referenced task/review policy.
- If Den is unavailable, report the failed operation and stop; do not reconstruct task state locally.

## Ownership

- `csharp/LoadingBay.Game` owns E1M1 policy, validation, game state, save meaning, facts, snapshots, and HUD projection. Keep values typed and named in `LoadingBayTuning`, definitions, and product records.
- `Rusty.Engine` is an immutable package. It generates the product composition below `obj`; do not add a composition project, `EngineProduct` assembly attribute, P/Invoke, exported entrypoint, generated bindings, or source project reference downstream.
- The matching runtime pack owns host integration, renderer/canvas, input, lifecycle, browser shell, and renderer preload. Use `rusty dev --project` for normal development. An Engine contributor may use `--engine-source /absolute/path` only as an explicit source override.
- TypeScript is the Angular DOM UI module. It exports `mountProductUi`, renders the read-only HUD, and does not evaluate gameplay, construct live state, configure a renderer, or own a loop/canvas.
- Keep authored E1M1 content, live C# runtime state, and transient presentation separate. Preserve the exact source and asset provenance in `docs/source-provenance.md`.

## Proof

- C# changes: `./scripts/verify-csharp-spine.sh` checks the semantic catalog, staged CoreCLR product, lifecycle exercise, and NativeAOT fidelity target.
- Browser-facing changes: build the Angular shell, run `./scripts/run-csharp-product.sh`, and obtain focused evidence of the affected Engine canvas/HUD/input continuation.
- Content/provenance changes: run the relevant deterministic content/provenance check; do not make the C# runtime parse source-shaped authoring data.
- `pnpm run certify:e1m1` is release/manual only and currently stalls at waypoint `[127,121]`. It is not passing certification or proof of complete E1M1 traversal.
