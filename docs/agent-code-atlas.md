# Agent code atlas

Use this page to find the owner before editing. It routes navigation; owning contracts and tests
remain authoritative.

| Concern                           | Owner paths                                                                                    | Focused proof                                          |
| --------------------------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| Game entities and services        | `rust/crates/loading-bay-game/src/*.rs`                                                        | `./scripts/verify-rust.sh`                             |
| Project admission and persistence | `stored_project.rs`, `project_*`, `save_game.rs`, `snapshot.rs`                                | focused Rust integration tests                         |
| Fixed input/game phases           | `game_loop.rs`, `player.rs`, `combat.rs`, `combat_resolution.rs`                              | game-loop, combat, and controller tests                |
| Native rendered product           | `src/bin/native-host.rs`                                                                       | `pnpm run verify:native`                               |
| Browser transport and HUD shell   | `src/bin/browser-host.rs`, `ts/packages/browser-shell`, `apps/loading-bay`                     | `pnpm run test:shell`, browser smoke                   |
| Studio adapter and bootstrap      | `studio_adapter/`, `.rusty-studio.json`                                                        | focused Studio adapter tests                           |
| Engine dependency boundary        | root manifests, `scripts/audit-boundary.mjs`                                                   | `pnpm run audit:boundary`                              |
| Durable ownership intent          | `docs/design.md`                                                                               | doc review and full gate                               |
| Source and asset provenance       | `docs/source-provenance.md`                                                                    | provenance review and content gate                     |

Do not add Engine renderer imports under `apps/`, `libs/`, `ts/`, or ordinary scripts. If native
rendering needs a new mechanism, first determine whether the named Rust adapter already exposes it;
otherwise route the reusable change upstream rather than copying bridge or renderer code here.
