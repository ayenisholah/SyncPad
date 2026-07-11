# Contributing to SyncPad

Thanks for your interest in SyncPad. The project is in active early
development, so the surface area changes quickly — opening an issue before a
large pull request will save you time.

## Development setup

Requirements: Rust (stable, with `rustfmt` and `clippy`), Node.js 22+, npm.

```sh
git clone https://github.com/ayenisholah/SyncPad.git
cd SyncPad
bash scripts/verify.sh
```

`scripts/verify.sh` runs the format check, clippy with warnings denied, the
full server test suite, and the frontend build and tests — it must pass
before every commit. On Windows you can also use:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\verify.ps1
```

## Testing

- `cargo test --workspace` runs the server suite; `npm test` in `web/` runs
  the frontend suite.
- Synchronization correctness is the project. The convergence fuzz harness
  (`server/tests/fuzz_convergence.rs`) is the most important test in the
  repository: changes to operation handling, the revision log, or the client
  state machine must keep it green and should extend it where behavior
  changes. CI runs fixed seeds; for longer local runs crank it up with
  `SYNCPAD_FUZZ_SEEDS=500 SYNCPAD_FUZZ_ROUNDS=400 cargo test -p
  syncpad-server --test fuzz_convergence`. A failure names its seed — when
  fixing a bug found this way, keep that seed as a regression case.
- The OT transform algebra comes from the `operational-transform` crate and
  is never reimplemented here; tests exercise the protocol around it
  (ordering, acks, resync, cursor transformation), not the transform math.
- Never weaken or delete an existing test to get to green.

## Commit conventions

- [Conventional Commits](https://www.conventionalcommits.org/) subject lines:
  `type(scope)?: summary`, type ∈ feat fix docs chore refactor test perf ci
  build style, ≤ 72 characters.
- One logical change per commit; CHANGELOG entries go under `[Unreleased]`.

## Proposing significant changes

Design-level changes (new dependencies, protocol behavior, architecture)
are recorded as ADR-lite entries in [docs/DECISIONS.md](docs/DECISIONS.md).
Open an issue or a PR adding an entry with Status `Proposed` and the
context/decision/consequences filled in; implementation starts once it is
`Approved`.

## Scope

SyncPad intentionally excludes accounts/auth/permissions, a database, a
document history UI, rich text, and mobile layout polish (see the
[engineering spec](docs/syncpad-engineering-doc.md)). PRs adding these will
be declined; feel free to discuss in an issue first.
