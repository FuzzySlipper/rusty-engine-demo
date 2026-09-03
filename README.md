# Loading Bay

Loading Bay is an ordinary C# Rusty Engine product for the committed Doom E1M1 Hangar closure. Its normal development lane is the packaged `Rusty.Engine` SDK and the matching `rusty dev` CoreCLR runtime pack. NativeAOT is a separate fidelity/release check, not the edit-run loop.

## Run the product

The ignored `.runtime/` directory receives the matched SDK feed and runtime pack for the selected development release. It contains Engine artifacts only; the repository does not carry an Engine host, browser shell, generated binding, or source checkout.

```bash
pnpm install --frozen-lockfile
pnpm run build:shell
./scripts/run-csharp-product.sh
```

The launcher stages `LoadingBay.Game` through the SDK and starts its CoreCLR bundle with `rusty dev`. It prints the local URL (normally `http://127.0.0.1:4394`). The runtime pack owns its browser shell, renderer preload, canvas, input, and lifecycle; Angular supplies the disposable DOM HUD module.

Engine contributors may explicitly select a source override with `./scripts/run-csharp-product.sh --engine-source /absolute/rusty-engine`. Ordinary product work must use the package and matching runtime pack instead.

## Architecture

- `csharp/LoadingBay.Game` owns typed E1M1 policy, product state, validation, facts, snapshots, saves, and the `loading-bay.hud.snapshot.v1` projection.
- The packaged SDK generates CoreCLR staging and NativeAOT composition below `obj`; Loading Bay has no checked composition project or native exports.
- Rusty Engine owns the host, update cadence, renderer/canvas, spatial and voxel mechanisms, content admission, persistence primitives, and browser runtime shell.
- `apps/loading-bay` exports `mountProductUi` for that shell and renders copied HUD data only. It does not run gameplay, create a renderer, or own a loop.
- E1M1 source artifacts and provenance remain under `content/doom-e1m1/`, `content/projects/`, and [docs/source-provenance.md](docs/source-provenance.md). The runtime admits the committed closure through Engine content services rather than parsing authoring source in C#.

## Focused proof

```bash
./scripts/verify-csharp-spine.sh  # catalog, staged CoreCLR build, lifecycle exercise, NativeAOT fidelity check
pnpm run audit:boundary           # packaged SDK/runtime ownership and retired-lane checks
```

Browser-facing work still needs focused visible evidence of the affected canvas/HUD/input path. Build or HTTP success alone is not browser acceptance. `pnpm run certify:e1m1` is release/manual only and currently stalls at waypoint `[127,121]`; do not treat it as a passing complete traversal.

## Documentation

- [Design and authority](docs/design.md)
- [Onboarding](docs/downstream-onboarding.md)
- [Extension recipes](docs/extension-recipes.md)
- [C# migration map](docs/code-migration-map.md)
- [E1M1 gameplay ledger](docs/doom-e1m1-gameplay-ledger.md)
- [Presentation frame](docs/presentation-frame.md)
- [Source provenance](docs/source-provenance.md)
