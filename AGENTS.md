# Rusty Engine Demo agent guidance

This repository is the external loading-bay game and reference consumer of Rusty Engine. It is not
an Asha compatibility project and it is not the place to generalize speculative Engine APIs.

- Keep gameplay object-centric: configuration/state lives on responsible entities; named Rust
  services and explicit host phases own behavior.
- Rust owns live gameplay state, validation, mutation, scheduling, facts, project persistence, and
  runtime snapshots.
- TypeScript owns immutable content composition, browser semantic-input capture, tooling, HUD state,
  and disposable product-shell presentation only. It does not import or configure the Engine
  renderer.
- Keep one unconditional, one-way `rusty-engine` facade dependency on the public `main` branch.
  Owner namespaces remain explicit (`rusty_engine::<owner>`), but downstream must consume the
  complete facade rather than selecting crates. The lock must stay current with upstream and a
  local sibling path is never a supported fallback.
- The native product owns its window, bounded mount region, frame timing, resource policy, semantic
  input mapping, picks, and game consequences. Call the Engine-owned Rust webview adapter; never
  reach through it to the private TypeScript bridge or packaged renderer artifact.
- Add genuinely game-specific semantics here without editing Engine vocabulary. Promote a smaller
  Engine seam only after another real consumer proves it reusable.
- Do not introduce a plugin registry, service locator, universal behavior IR, generic method-name
  bridge, replay/certification spine, or live TypeScript gameplay authority.
- Keep authored project content distinct from live runtime snapshots and transient presentation.
- Preserve exact source and asset provenance in `docs/source-provenance.md`.
- Follow the concrete owner/test/upstream gates in `docs/extension-recipes.md`; do not infer a
  plugin or generic bridge from the examples.
- Run `./scripts/verify-rust.sh` for Rust-only iteration, `pnpm run verify:native` for renderer-host
  work, and `pnpm run verify` for every product-visible or cross-language change.
