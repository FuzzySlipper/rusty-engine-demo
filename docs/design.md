# Loading Bay design

Loading Bay is a single C# NativeAOT product that exercises Rusty Engine with the committed Doom E1M1 content closure. The product is intentionally narrow: it is a useful downstream reference, not a general game framework or compatibility facade.

## Authority boundary

`LoadingBayProduct` and `LoadingBaySession` own product meaning. Their typed records and services own E1M1 state, validation, facts, save identity, snapshots, and readouts. `LoadingBayTuning` is the observable, named location for gameplay calibration: health/armor policy, movement, look, gravity, jump, camera, spatial scale, landmarks, perception, and effects. `LoadingBayDefinitions` owns the typed product item and weapon definitions. A value that changes gameplay belongs in one of those C# product domains, not in Angular, a host mapping, or an unlabelled literal.

Rusty Engine owns generic mechanisms: admitted product lifecycle and cadence, input delivery, content/voxel admission, spatial collision and character steps, camera and perception, presentation/animation, UI streams, persistence primitives, diagnostics, and the renderer/canvas. Loading Bay uses only the public Engine C# and Product Browser Host surfaces.

TypeScript owns browser-only presentation. The Angular shell captures semantic input, asks the public host to mount the product, preloads immutable renderer resources, and renders the immutable `loading-bay.hud.snapshot.v1` stream. It cannot mutate or infer authoritative gameplay state.

## Product path

```text
Angular semantic input / read-only HUD
              │
public Product Browser Host
              │
NativeAOT LoadingBayProduct
              │
typed Loading Bay session and tuning ─── Rusty Engine mechanisms
              │                                │
committed E1M1 content closure ────────── Engine renderer/canvas
```

The Product Browser Host is a development adapter, not product authority. It
mounts the NativeAOT product, owns the Engine canvas and cadence, and exposes
the bounded semantic input/UI surface used by the Angular shell.

## Content and observability

The authoritative shipped content is the committed E1M1 closure under `content/`; source provenance remains in [source-provenance.md](source-provenance.md). C# validates and opens the precise project, voxel, and asset-catalog entries through Engine content services. The offline authoring forge may derive assets and manifests, but it is not a live gameplay evaluator.

The HUD projection carries product readouts such as health, armor, ammunition, generation, admitted step, facts/drop telemetry, traversal tuning, and the bounded exit-visibility presentation state. These make the demo inspectable without moving authority into TypeScript. Live-debug operations likewise target named product tracks and retain typed receipts/facts.

## Demonstrated scope and limits

Current focused evidence demonstrates NativeAOT construction, Engine-hosted E1M1 rendering, a single Engine canvas, the structured HUD stream, and realtime semantic movement continuation. The browser capture currently retains a black horizontal band, and repeated pointer-locked fire may be ignored after the initial shot. The C# lifecycle exercise covers its named state/save/fact receipts; this evidence does not claim complete player traversal or every authored interaction.

`pnpm run certify:e1m1` remains manual/release work and currently stalls at `[127,121]`. The active limitation belongs in the project limitation record until a new observed run replaces it.
