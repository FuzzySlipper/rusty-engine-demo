# Extending Loading Bay

Start at the named owner. Loading Bay is one E1M1 product, so an extension must not reactivate a legacy fixture or create a parallel gameplay/renderer authority.

## Add game semantics

Put new combat, inventory, progression, enemy, door, hazard, or save meaning in the responsible Rust entity/service and fixed game phase. Define the content shape in TypeScript only when it is immutable authoring input, then admit and evaluate it in Rust. Add a closed semantic command only when the product service needs a new player intent; do not add a generic RPC command.

## Add an authored gameplay-program primitive

First add the primitive's fact/predicate or operation meaning and policy handling
in Rust, including its candidate transaction effects and focused resolver proof.
Then add the matching closed immutable TypeScript builder/type in
`gameplay/authoring/src/authoring/` and materialize the package/project. Do not
add a method-name string, callback, registry, arbitrary expression, or
TypeScript evaluator. Another downstream should copy this boundary—its own
Rust-owned vocabulary and authoring builders—not Loading Bay's hitscan names
or Rusty Dagger's RPG grammar. This is not a universal Engine behavior
language: keep object references on admitted components, bind named programs to
those objects, and let the owning Rust service execute/commit the candidate.

## Add E1M1 content

Author the change in `ts/packages/doom-e1m1-authoring` and materialize the sole canonical project at `content/projects/doom-e1m1.project.json`. Keep collision, navigation, triggers, hitboxes, and gameplay ownership explicit in Rust/project components; a mesh or texture is presentation only. Place new shipped E1M1 source assets under `content/doom-e1m1/`, record their exact provenance in `docs/source-provenance.md`, and extend deterministic content checks.

## Choose a standard route

Use a `gameplay-standard` preset when the product needs its ordinary mechanics
shape and can compose its public catalog/component fragments directly. Loading
Bay's vitality setup is the example: the standard actor fragment is merged
with Doom's armor/item entries in `gameplay/src/mechanics.rs`, and explosive
props use the standard destructible integrity track through the same named
damage/health/snapshot services. The current destructible preset is
fixed-capacity; admission rejects an incompatible future prop rather than
locally widening it. Do not wrap preset fragments in a local generic preset
framework.

Use a typed standard extension when the product has a small immutable policy
that Engine must carry but not interpret. The `e1m1-standard-vitality`
TypeScript package and `gameplay/src/standard_vitality.rs` show the full path:
generated DSL, canonical package, standard admission, product compiler, named
Rust policy consumed by normal gameplay admission. Keep hitscan, drops,
encounters, pickups, and consequences as
product-specific typed vocabularies rather than forcing them into this artifact.

## Extend a shell

Browser changes belong in the Angular shell or `browser-shell`, translating semantic input to the existing service/session contract. Tauri changes belong in the typed in-process adapter. Both must use the public Engine application-host; neither may import a private renderer bridge, create a canvas, or own a frame loop.

For developer commands, expose the public Engine marker through
`CommandBindings::expose_borrowed`, enqueue the generated host request, and
call `dispatch_borrowed` only where the live Rust owner is already available at
the product safe point. Use `HostCommandDiscovery::from_bindings` and
`map_command_response`; do not recreate envelope validation, correlation
history, discovery descriptors, or response DTOs locally. Keep only typed
product payload/result adapters and bounded transport queues. Loading Bay's
browser WebSocket and Tauri IPC are examples of thin adapters over that same
port, while `ts/packages/browser-shell/src/developer-command.ts` shows public
generated-client schemas, cancellation, and application-host shell wiring.
Loading Bay supplies its product command schema in the base schema map because
that executable command already appears in authoritative host discovery; adding
the same descriptor again as a client extension would create a duplicate
command identity. A developer play command must await the normal product
outcome rather than reporting queue admission as gameplay completion.

## Proof

Run `./scripts/verify-rust.sh` for Rust-only work and `pnpm run verify` for product-visible or cross-language work. Add focused checks for the touched surface:

```bash
pnpm run test:shell
pnpm run test:engine-route
pnpm run test:platform
pnpm run audit:boundary
pnpm run smoke:e1m1
pnpm run verify:tauri
```

The full E1M1 certifier is release/manual work and currently stalls at `[127,121]`; do not use it as passing proof.
