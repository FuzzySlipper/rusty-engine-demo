# Loading Bay design

Loading Bay is a single ordinary C# product that exercises Rusty Engine with the committed Doom E1M1 content closure. The product is intentionally narrow: it is a downstream reference, not a general game framework or compatibility facade.

## Authority boundary

`LoadingBayProduct` and `LoadingBaySession` own product meaning. Their typed records and services own E1M1 state, validation, facts, save identity, snapshots, and readouts. `LoadingBayTuning` is the named location for gameplay calibration; `LoadingBayDefinitions` owns typed product item and weapon definitions. A gameplay value belongs in those C# domains, not in Angular, an input mapping, or an unlabelled literal.

Rusty Engine owns admitted lifecycle and cadence, input delivery, content/voxel admission, spatial collision and character steps, camera and perception, presentation/animation, UI streams, persistence primitives, diagnostics, renderer/canvas, and browser shell. The immutable SDK exposes the supported C# services and generates product binding below `obj`.

Angular is a DOM UI module. The runtime shell imports its `mountProductUi` entry only after mounting the Engine host. It consumes the copied `loading-bay.hud.snapshot.v1` projection; it does not mount a host, preload renderer resources, create a canvas, mutate gameplay, or advance time.

## Product path

```text
Angular read-only HUD module
              │
matched runtime-pack browser shell
              │
CoreCLR LoadingBayProduct through packaged SDK
              │
typed Loading Bay session and tuning ─── Rusty Engine mechanisms
              │                                │
committed E1M1 content closure ────────── Engine renderer/canvas
```

CoreCLR is the normal development lane. The SDK stages the loose Product directory and the runtime pack verifies its own matched ABI identity before construction. NativeAOT is generated from the same ordinary product only when `VerifyRustyEngineAot` is deliberately requested.

## Content and observability

The authoritative shipped content is the committed E1M1 closure under `content/`; source provenance remains in [source-provenance.md](source-provenance.md). C# validates and opens the precise project, voxel, and asset-catalog entries through Engine content services. The offline authoring forge may derive assets and manifests, but it is not a live gameplay evaluator.

The HUD projection carries product readouts such as health, armor, ammunition, generation, admitted step, facts/drop telemetry, traversal tuning, and bounded exit-visibility state. These make the demo inspectable without moving authority into TypeScript.

Current visible evidence retains a black horizontal band, and repeated pointer-locked fire may be ignored after the initial shot. The lifecycle exercise covers named state/save/fact receipts; it does not claim complete player traversal or every authored interaction. `pnpm run certify:e1m1` remains manual/release work and currently stalls at `[127,121]`.
