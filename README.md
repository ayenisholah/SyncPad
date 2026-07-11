# SyncPad

Real-time collaborative code editor — Rust, WebSockets, and operational
transforms.

SyncPad is a Google-Docs-style editor for code: anyone with the link edits the
same document simultaneously in Monaco, a Rust server merges concurrent edits
with operational transforms (OT) so no keystrokes are lost, and there is no
database — documents live in memory with periodic snapshots to disk.

## Status

Pre-alpha. The sync core is in place and fuzz-verified: each live document
is owned by one tokio task, concurrent operations are transformed against
the revision log (via the `operational-transform` crate) and broadcast to
peers with acks and a resync recovery path, and a seeded fuzz harness
drives simulated clients through randomized concurrent editing to assert
byte-identical convergence on every run. Presence (server-assigned names
and colors, join/leave events, roster in `init`) tracks who is editing.
Next up: snapshots and document lifecycle, then the editor frontend.

| Area | Status |
|---|---|
| Document creation (`POST /api/docs`, slug URLs) | Done |
| WebSocket connect with `init` document state | Done |
| Wire-protocol message types (tested codecs) | Done |
| Frontend shell (landing page, editor route) | Done |
| Per-document task: presence, broadcast | Done |
| Server-side OT against the revision log | Done |
| Convergence fuzz harness | Done |
| Snapshots + idle expiry (no database) | Planned |
| Monaco editor + client OT state machine | Planned |
| Live cursors, selections, presence list | Planned |
| Language picker | Planned |
| Latency instrumentation (status-bar p50) | Planned |
| Deployment (Docker, single instance) | In progress |

No performance numbers are claimed until they are measured and committed with
the measurement methodology.

## Why

Concurrent editing is a consistency problem. When two people type at the same
position at the same instant, a naive "last write wins" server silently
destroys one of the edits. The two established solutions are **operational
transforms** — a central server transforms concurrent operations against each
other so they compose — and **CRDTs** — merge-anywhere data structures that
need no central authority.

SyncPad deliberately takes the OT-with-central-server approach. A server is
already in the topology (it serves the app and brokers WebSockets), OT keeps
per-document memory small, and the resulting architecture is easy to reason
about: one authoritative document per room, a revision log, server-side
transformation of concurrent operations, and broadcast to every connected
editor. CRDTs shine when there is no authoritative node or when offline-first
merging matters; that is not this product.

The transform algebra itself comes from the
[`operational-transform`](https://crates.io/crates/operational-transform)
crate rather than a hand-rolled implementation — convergence correctness
belongs in a tested library. What SyncPad owns is the protocol around it:
revision ordering, the client state machine, cursor transformation, presence,
and recovery.

## Design

- **One tokio task per document.** All mutation of a document is
  single-threaded inside its task — no locks around OT state. Connections
  talk to the task over channels; broadcasts fan out to subscribers.
- **No database, by design.** Documents live in memory. Dirty documents are
  snapshotted to per-document JSON files every 30 seconds and on graceful
  shutdown, and lazily reloaded on first access after a restart. Documents
  idle for more than 24 hours expire.
- **The link is the capability.** There are no accounts: an unguessable
  8-character slug (32-character alphabet, ~10^12 possibilities) is the only
  access control, with no enumeration endpoint and per-connection rate and
  size limits.

See [docs/syncpad-engineering-doc.md](docs/syncpad-engineering-doc.md) for the
full architecture, protocol, and requirements.

## Development

Requirements:

- Rust (stable) with `rustfmt` and `clippy`
- Node.js 22+ and npm
- Bash or Git Bash if you want to run `scripts/verify.sh`

Run the project verification loop (format check, clippy with warnings denied,
server tests, frontend build and tests):

```sh
bash scripts/verify.sh
```

On Windows, the PowerShell variant may need an execution policy bypass:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\verify.ps1
```

Run the server and frontend during development:

```sh
cargo run -p syncpad-server        # server on http://127.0.0.1:8090
cd web && npm run dev              # Vite dev server proxying /api and /ws
```

## Deployment

SyncPad ships as a single container behind a reverse proxy — in-memory documents
pin a document to one process, so it runs as one instance by design. A
multi-stage `Dockerfile` builds the frontend and the server into a small
distroless image that serves the SPA, the API, and the WebSocket from one
origin.

```sh
docker compose -f deploy/docker-compose.yml up -d --build
```

The container listens on `127.0.0.1:8090` with a volume for `/data` (the
snapshots); an nginx server block fronts it. A graceful stop (SIGTERM) flushes
dirty documents before exit. Continuous delivery is handled by GitHub Actions:
it builds the image, publishes it to the GitHub Container Registry, and — on a
version tag or a manual run — deploys to the host over SSH (`docker compose
pull`). See [deploy/README.md](deploy/README.md) for the full VPS setup,
including CI secrets, the nginx config, and TLS.

## Roadmap

Ordered so that sync correctness is proven before any editor UI exists:

1. Snapshots, idle expiry, per-connection limits.
2. Monaco editor and the client OT state machine; two browsers typing.
3. Live cursors and selections, presence bar, language picker, latency
   readout.
4. Deployment, then measured latency and concurrency numbers.

## Scope

SyncPad deliberately does not include accounts, authentication, or
permissions (anyone with the link edits — that is the product), a database
(in-memory documents with snapshot files are the persistence story), a
document history UI, rich text, or mobile layout polish. These are documented
as future work in the engineering spec so the core synchronization story
stays focused.

## Repository Map

| Path | Purpose |
|---|---|
| `server/` | Rust server (axum): HTTP, WebSockets, document registry |
| `web/` | Vite + React + TypeScript frontend |
| `docs/syncpad-engineering-doc.md` | Engineering spec |
| `docs/DECISIONS.md` | Architecture decision records |
| `scripts/verify.*` | Local build, lint, and test entrypoints |

## License

[MIT](LICENSE) (c) 2026 Shola Ayeni
