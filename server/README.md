# Rust server

`syncpad-server` is an Axum/Tokio application. It exposes `POST /api/docs`,
`GET /ws/{doc_id}` (WebSocket upgrade), and a static SPA fallback. There is no
document listing endpoint.

Each active document has one Tokio task and channel-based handle. That task
serializes joins, leaves, operations, cursor changes, and language changes;
there are no locks around document OT state. WebSocket JSON message types are
defined and tested in `src/protocol.rs`; see [architecture](../docs/architecture.md)
for the lifecycle.

## Configuration

| Variable | Default | Meaning |
|---|---:|---|
| `PORT` | `8090` | HTTP/WebSocket listen port (`0.0.0.0`). |
| `SYNCPAD_STATIC_DIR` | `web/dist` | Built SPA directory. |
| `SYNCPAD_DATA_DIR` | `data` | Snapshot directory; `/data` in the image. |
| `SYNCPAD_SNAPSHOT_SECS` | `30` | Dirty snapshot interval. |
| `SYNCPAD_DOC_TTL_SECS` | `86400` | Idle document expiry. |
| `SYNCPAD_REAP_SECS` | `3600` | Expiry scan interval. |
| `RUST_LOG` | `info` | tracing filter. |

Snapshot writes are atomic temp-file renames and hydrate lazily. SIGINT or
SIGTERM stops periodic jobs and flushes dirty documents. Docker grants a
10-second stop grace period. Abrupt termination may lose one snapshot interval.

Fixed limits are 64 KiB per WebSocket message, 100 ops/s per connection with a
one-second burst, and 10 distinct live documents per IP. These are abuse guards,
not authentication.

## Tests

Run `cargo test --workspace`. Unit tests cover document, registry, limits,
protocol, and snapshot internals. Integration suites cover HTTP/WS behavior,
snapshots, expiry, and randomized convergence. Increase fuzz work with
`SYNCPAD_FUZZ_SEEDS` and `SYNCPAD_FUZZ_ROUNDS`; preserve a failing seed as a
regression case. `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` are CI gates.
