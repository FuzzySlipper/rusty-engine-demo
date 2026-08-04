# Rusty Engine revision updates

[`engine-source.json`](../engine-source.json) is the exact-certification declaration of the active
Rusty Engine repository and commit. Rolling development is declared separately in
[`engine-development.json`](../engine-development.json): it follows the canonical public
`refs/heads/main` line, reports the resolved SHA, and does not promise compatibility.

Run the non-mutating consistency check before working on an Engine-dependent change:

```console
./scripts/engine-revision check
./scripts/engine-revision certify check
```

For rolling development, resolve and report one current provider SHA:

```console
./scripts/engine-revision dev sync --json
./scripts/engine-revision dev check
```

`dev sync` updates all active carriers as one coherent resolution and writes the ignored
`.engine-development/resolution.json` report. Use `--report-only` to inspect a public ref without
changing exact carriers, or `--worktree /absolute/engine-root` to inspect an explicitly selected
local Engine checkout. A dirty local worktree is report-only. The report is operational development
evidence, not an exact certification pin.

To inspect a proposed public revision without changing the caller's checkout:

```console
./scripts/engine-revision update <sha> --dry-run
```

The updater proves the commit is fetchable from the canonical public repository, rejects dirty
active carriers, creates a disposable detached worktree at the caller's current `HEAD`, rewrites
only the declared carriers, regenerates the Cargo and pnpm lockfiles with the repository-pinned
toolchain, runs the revision and boundary checks, and prints the exact scoped diff. It removes the
worktree whether preparation succeeds or fails.

Apply that same validated operation with:

```console
./scripts/engine-revision update <sha>
```

Rollback uses the same checked path rather than a separate restore mechanism. Preview the prior
known-good public commit, then apply it:

```console
./scripts/engine-revision update <previous-sha> --dry-run
./scripts/engine-revision update <previous-sha>
```

Before applying the candidate diff, the updater verifies that the caller's `HEAD` and active
carriers have not changed. Unrelated dirty files remain untouched. A failure leaves the caller's
active carriers unchanged. The command deliberately does not commit, push, change protocol
fixtures, rewrite historical provider/evidence SHAs, or perform upstream reverse-consumer
certification.

The active carrier set is closed and tested:

- `engine-source.json`
- `Cargo.toml` and `Cargo.lock`
- the root and browser-shell `package.json` files
- `pnpm-workspace.yaml` and `pnpm-lock.yaml`

Every other repository `package.json` and `Cargo.toml` is discovered and audited as an adjacent
dependency manifest. Those manifests may consume declared Rust workspace dependencies with
`.workspace = true`, but cannot introduce another direct Engine source or package revision.

If exact `certify check` reports a missing, renamed, mixed-revision, floating, path, sibling,
stale-lock, or unexpected Engine source, preserve any intentional work first and use the update
command to repair the whole set. Development mode may instead expose a compile or protocol
incompatibility against the newly resolved SHA; that failure is useful feedback and must not be
hidden by retaining an old exact pin.
