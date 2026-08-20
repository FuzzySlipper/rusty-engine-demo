# Extending Loading Bay

Start at the named owner. Loading Bay is one E1M1 product, so an extension must not reactivate a legacy fixture or create a parallel gameplay/renderer authority.

## Add game semantics

Put new combat, inventory, progression, enemy, door, hazard, or save meaning in the responsible Rust entity/service and fixed game phase. Define the content shape in TypeScript only when it is immutable authoring input, then admit and evaluate it in Rust. Add a closed semantic command only when the product service needs a new player intent; do not add a generic RPC command.

## Add E1M1 content

Author the change in `ts/packages/doom-e1m1-authoring` and materialize the sole canonical project at `content/projects/doom-e1m1.project.json`. Keep collision, navigation, triggers, hitboxes, and gameplay ownership explicit in Rust/project components; a mesh or texture is presentation only. Place new shipped E1M1 source assets under `content/doom-e1m1/`, record their exact provenance in `docs/source-provenance.md`, and extend deterministic content checks.

## Extend a shell

Browser changes belong in the Angular shell or `browser-shell`, translating semantic input to the existing service/session contract. Tauri changes belong in the typed in-process adapter. Both must use the public Engine application-host; neither may import a private renderer bridge, create a canvas, or own a frame loop.

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
