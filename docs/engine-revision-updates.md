# Rusty Engine revision updates

[`engine-source.json`](../engine-source.json) is the sole declaration of the active Rusty Engine
repository and commit. The repository URL is canonical and the commit is one lowercase
40-character hexadecimal SHA. Cargo, renderer, and Studio dependency declarations and both lock
files must all agree with it.

Run the non-mutating consistency check before working on an Engine-dependent change:

```console
./scripts/engine-revision check
```

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

If `check` reports a missing, renamed, mixed-revision, floating, path, sibling, stale-lock, or
unexpected Engine source, preserve any intentional work first and use the update command to repair
the whole set. Do not hand-edit only one dependency surface.
