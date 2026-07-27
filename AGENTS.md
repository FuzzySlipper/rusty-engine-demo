# Rusty Engine Demo agent guidance

This repository is the external loading-bay game and reference consumer of Rusty Engine. It is not
an Asha compatibility project and it is not the place to generalize speculative Engine APIs.

- Keep gameplay object-centric: configuration/state lives on responsible entities; named Rust
  services and explicit host phases own behavior.
- Rust owns live gameplay state, validation, mutation, scheduling, facts, project persistence, and
  runtime snapshots.
- TypeScript owns immutable content composition, browser input capture, typed projection, tooling,
  and disposable presentation only.
- Keep the Rusty Engine dependency one-way and pinned to an exact public Git revision. Never make a
  local sibling path the only supported build.
- Add genuinely game-specific semantics here without editing Engine vocabulary. Promote a smaller
  Engine seam only after another real consumer proves it reusable.
- Do not introduce a plugin registry, service locator, universal behavior IR, generic method-name
  bridge, replay/certification spine, or live TypeScript gameplay authority.
- Keep authored project content distinct from live runtime snapshots and transient presentation.
- Preserve exact source and asset provenance in `docs/source-provenance.md`.
- Follow the concrete owner/test/upstream gates in `docs/extension-recipes.md`; do not infer a
  plugin or generic bridge from the examples.
- Run `./scripts/verify-rust.sh` for Rust-only iteration. Use `pnpm run verify` for every
  product-visible or cross-language change.
