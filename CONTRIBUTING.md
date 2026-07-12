# Contributing to SyncPad

Sync correctness and honest operational claims come first. Discuss a large or
scope-changing proposal in an issue before investing in implementation.

## Setup

Install stable Rust with `rustfmt` and `clippy`, Node.js 22+, npm, and Git.

```sh
git clone https://github.com/ayenisholah/SyncPad.git
cd SyncPad
./scripts/setup.sh
bash scripts/verify.sh
```

Windows users can run `powershell -ExecutionPolicy Bypass -File
scripts\setup.ps1` and `scripts\verify.ps1`. Development uses two terminals:
`cargo run -p syncpad-server` and `cd web && npm run dev`.

## Branches and commits

Branch from current `main` and use a short descriptive branch name. Do not mix
unrelated changes. Commit subjects follow Conventional Commits:
`type(scope)?: summary`, where type is `feat`, `fix`, `docs`, `chore`,
`refactor`, `test`, `perf`, `ci`, `build`, or `style`; keep the subject within
72 characters. Add user-visible changes under `CHANGELOG.md` `[Unreleased]`.

## Test matrix

| Layer | Command | Covers |
|---|---|---|
| Rust format/lint | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings` | Style and warnings |
| Rust unit/integration | `cargo test --workspace` | Protocol, tasks, limits, API/WS, snapshots, reaper |
| Convergence fuzz | `cargo test -p syncpad-server --test fuzz_convergence` | Random concurrent clients and revisions |
| Frontend unit | `cd web && npm run test:unit` | OT state, operations, connection, routes, sharing |
| Frontend build/docs | `cd web && npm run check` | Types, bundle, unit tests, local docs consistency |
| Browser E2E | `cd web && npm run test:e2e` | Convergence, cursors, language, sharing, UI |
| Full local gate | `bash scripts/verify.sh` | Repository CI-equivalent checks |

Control longer fuzz runs with `SYNCPAD_FUZZ_SEEDS` and
`SYNCPAD_FUZZ_ROUNDS`, for example:

```sh
SYNCPAD_FUZZ_SEEDS=500 SYNCPAD_FUZZ_ROUNDS=400 \
  cargo test -p syncpad-server --test fuzz_convergence
```

A failure prints its seed. Preserve it as a regression test. Never weaken a
test to make a change pass. E2E requires Chromium (`npx playwright install
chromium`), port 8090, a Rust toolchain, and network access for Monaco/fonts.

## Documentation and design

Run `cd web && npm run docs:check` after Markdown or script changes. Regenerate
committed media with `npm run docs:assets`; capture creates disposable local
documents. Do not put production/private content in examples or captures.

New runtime or build dependencies need rationale and must be pinned where
reproducibility matters. Protocol behavior, architecture, scope, and material
dependency decisions require an ADR-lite entry in
[docs/DECISIONS.md](docs/DECISIONS.md), proposed before implementation. Keep the
[engineering specification](docs/syncpad-engineering-doc.md), architecture,
measurements, and operational guide authoritative in their domains.

## Pull requests

Explain the problem and approach, link issues/ADRs, enumerate tests run, attach
UI evidence when relevant, and call out migration, deployment, security, or
rollback effects. Keep CI green. PRs must not claim unmeasured performance or
add excluded scope without an approved design change.
