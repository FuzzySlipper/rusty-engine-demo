# Extending Loading Bay

Loading Bay accepts only E1M1 product work. Start at the owner below and keep
the product path aligned with the ownership table.

## Change gameplay policy

Put a game-specific rule in `csharp/LoadingBay.Game` beside its typed domain. Add or extend a named record/definition, keep tunable numeric policy in `LoadingBayTuning`, and expose a bounded fact/readout when the player or future agent needs to observe the result. Use `LoadingBaySession` for state/validation/save meaning and Engine mechanics services for generic inventory, tracks, or effects.

Do not encode a rule in an Angular component, input mapping, host CLI flag, or generic command registry. Do not introduce a product scheduler, spatial model, renderer, replay model, or TypeScript evaluator.

## Need an Engine mechanism

Use the public Engine C# capability first: application admission/cadence, content, spatial, voxel, camera, perception, presentation/animation, UI stream, persistence, or diagnostics. If the necessary neutral capability is absent, document the concrete downstream need and route the narrow seam upstream. Do not add a private bridge, P/Invoke, generated-artifact import, duplicate validation, or local workaround that makes two authorities.

## Change E1M1 content

Keep committed runtime artifacts under `content/` and record every source or derived asset change in [source-provenance.md](source-provenance.md). The C# product must admit the committed closure via Engine content services; do not teach it to parse source-shaped authoring documents. Update a typed C# reference only when the product genuinely uses the changed artifact, and keep its identity/coordinate/value visible in the relevant tuning or content owner.

The offline content forge may regenerate deterministic assets and manifests. It
is not a live gameplay path; gameplay policy remains in the typed C# product
domains.

## Change the browser shell

Use the public Product Browser Host and `@rusty-engine/application-host`. Angular may translate physical events into semantic input and render copied, immutable HUD data. Engine owns the sole canvas, renderer lifetime, and realtime loop. Keep UI failure/loading states within the bounded product frame; controls outside that frame must not become gameplay input.

## Proof

Run `./scripts/verify-csharp-spine.sh` for product changes. Add focused browser evidence for a browser-facing change and focused deterministic/provenance evidence for content work. Record an unfinished visible behavior in the project's known-limitations record; do not claim complete E1M1 traversal from a build, HTTP response, or the currently stalled manual certifier.
