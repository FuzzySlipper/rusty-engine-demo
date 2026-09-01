# Loading Bay

Loading Bay is a C# NativeAOT reference consumer of [Rusty Engine](https://github.com/FuzzySlipper/rusty-engine). It demonstrates one supported authored experience: the Doom E1M1 Hangar recreation. The repository has one product path and one Engine-owned canvas.

## Run the product

The product expects the adjacent Engine checkout at `../rusty-engine`, Node/pnpm for the Angular shell, .NET for NativeAOT, and Rust for the Engine development runtime.

```bash
pnpm install --frozen-lockfile
pnpm run build:shell
./scripts/run-csharp-product.sh
```

Open the URL printed by the development host (normally `http://127.0.0.1:4394`). The host loads the NativeAOT `LoadingBayProduct`, Engine mounts its one canvas, and Angular mounts the read-only HUD over the Engine UI context.

## Architecture

- `csharp/LoadingBay.Game` is the product authority. It owns typed E1M1 policy, product state, validation, facts, snapshots, saves, and the `loading-bay.hud.snapshot.v1` projection.
- `csharp/LoadingBay.NativeProduct` exposes that product through Rusty Engine's generated NativeAOT boundary.
- Rusty Engine owns the public Product Browser Host, update cadence, input drain, renderer/canvas, spatial and voxel mechanisms, content admission, and persistence primitives.
- `apps/loading-bay` captures semantic input and renders only disposable presentation. It receives immutable UI projection data; it does not run gameplay.
- E1M1 source artifacts and provenance remain under `content/doom-e1m1/`, `content/projects/`, and [docs/source-provenance.md](docs/source-provenance.md). The runtime admits the committed closure through Engine content services rather than parsing authoring source in C#.

## Observable HUD and current limits

The HUD is a structured, read-only projection from `LoadingBay.Game`. It
surfaces typed health/armor, ammunition, generation, admitted-step, fact/drop,
world, and named tuning readouts so gameplay values can be inspected and
tuned without moving authority into TypeScript.

The current focused browser continuation shows the Engine canvas and live HUD,
but retains a black horizontal band in the frame. After the initial focused
shot, repeated fire while pointer-locked may also be ignored. These are known
visible limitations, not alternate product paths or proof of complete E1M1
traversal.

## Focused proof

```bash
./scripts/verify-csharp-spine.sh  # managed build, lifecycle exercise, NativeAOT publish
pnpm run build:shell              # retained Angular shell
./scripts/run-csharp-product.sh   # C# product through the public browser host
```

For browser-facing work, capture a bounded interactive continuation appropriate to the change: one Engine canvas, active HUD stream, and the affected semantic input/readout. Build or HTTP success alone is not visible product proof.

`pnpm run certify:e1m1` is an optional release/manual traversal route. It currently stalls at waypoint `[127,121]`; do not describe it as passing or use it to claim complete E1M1 gameplay.

## Documentation

- [Design and authority](docs/design.md)
- [Onboarding](docs/downstream-onboarding.md)
- [Extension recipes](docs/extension-recipes.md)
- [C# migration map](docs/code-migration-map.md)
- [E1M1 gameplay ledger](docs/doom-e1m1-gameplay-ledger.md)
- [Presentation frame](docs/presentation-frame.md)
- [Source provenance](docs/source-provenance.md)
