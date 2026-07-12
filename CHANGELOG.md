# Changelog

All notable changes to SyncPad are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) ·
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).
Changes accumulate under **[Unreleased]** and roll into a version at each
release.

## [Unreleased]

### Added

- A reproducible production-measurement harness and manual GitHub Actions
  workflow. It measures the public HTTPS/WSS path separately from a VPS-local
  capacity ramp, reports acknowledgement rate, throughput, p50/p95 latency,
  disconnects, and convergence failures, and preserves raw JSON results as
  workflow artifacts without weakening production abuse limits.
- First measured capacity result: the production server sustained at least 200
  collaborative sessions (400 WebSocket clients) and 400 acknowledged ops/s
  with no errors, disconnects, or convergence failures. This is documented as a
  tested lower bound; public-path latency remains unpublished pending its
  separate workflow artifact.
- Measured the full public HTTPS/WSS path across three valid 60-second runs:
  median remote-apply latency was p50 349 ms and p95 1,434 ms, with 100%
  acknowledgements and zero errors, disconnects, or convergence failures. The
  harness now waits for both replicas to reach the same settled revision before
  checking convergence on higher-latency links.
- Share code samples (D-008): a Share panel renders the current selection (or the
  whole document) as a branded, syntax-highlighted image — download it, copy it
  to the clipboard, or post to X, LinkedIn, or Reddit with a link back to the
  document. Highlighting reuses Monaco's colorizer and rendering is entirely
  client-side, so documents are never exposed to crawlers.
- Redesigned interface (spec §8.1): a refreshed dark violet theme with the Inter
  typeface, a subtle grid backdrop, and glassy panels. The landing page leads
  with the logo, a protocol badge, and value points; the editor top and status
  bars are tidied and the Monaco surface is themed to match the app background.
- Site branding and metadata: a SyncPad logo/favicon set, a 1200×630 Open Graph
  / Twitter card image, a web app manifest, and a full metadata head
  (description, Open Graph, Twitter `summary_large_image`, and a JSON-LD
  `SoftwareApplication` block). Documents are kept out of search — `robots.txt`
  disallows `/d/`, the sitemap lists only the landing page, and doc routes set a
  runtime `noindex`. Brand rasters are generated from SVG sources by
  `npm run gen:assets` and committed, so the production build needs no image
  tooling.

- Deployment (§12): a multi-stage `Dockerfile` builds the frontend and server
  into a small distroless image that serves the SPA, API, and WebSocket from one
  origin, run via `deploy/docker-compose.yml` behind an nginx reverse proxy
  (config and setup in `deploy/`). The server now shuts down gracefully on
  SIGTERM (flushing snapshots), and resolves the real client IP from a
  forwarded header so the per-connection limits stay per-user behind the proxy.
- Continuous delivery: a Container Image workflow builds and publishes the image
  to the GitHub Container Registry (`edge` from `main`, plus `v*` tag and commit
  SHA) on every push, and a separate one-click Deploy Production workflow ships
  the deploy bundle to the host over SSH and rolls the container (`docker compose
  pull` + `up -d`). Deploy is manual only, selects any published tag (rollback is
  the same click on an older one), and reads its VPS credentials from a GitHub
  `production` environment.
- Editor chrome (FR7, G3, G5, §8.1): a language picker in the top bar changes
  syntax highlighting for every window (server-validated against an allowlist,
  broadcast to all, and persisted in the snapshot), a presence bar shows who
  else is in the document as colored avatars, and the status bar shows the live
  op→apply latency alongside the connection state and revision.
- Live remote cursors and selections (FR5): each participant's caret and
  selection appear in every other editor, colored and name-tagged. The server
  tracks each cursor and transforms it through every accepted operation so it
  stays at the right character offset as the text shifts, broadcasts cursor
  updates to peers, seeds a new connection with existing cursors, and drops a
  cursor when its author leaves. Clients report their caret throttled to 50 ms
  and render remote cursors as Monaco decorations. Also moved the local dev and
  end-to-end server port to 8090.
- End-to-end convergence tests (Playwright): two browsers editing one document
  with concurrent same-offset typing converge byte-identical, a late joiner is
  seeded with the current content, and edits relay both directions. Run with
  `npm run e2e` (builds the frontend, boots the server against it, drives
  Chromium) and in CI on every push. The editor also buffers outgoing
  operations during IME composition so half-composed input is never sent.
- Collaborative editor client (FR3, FR4): a Monaco editor wired to the sync
  protocol through an operational-transform client. The client operation
  algebra (`apply`/`compose`/`transform`) is a compact TypeScript port matching
  the server's wire format and Unicode-scalar-value counting, guarded by a TP1
  property test over randomized concurrent edits. The ot.js-style state machine
  (synchronized / awaiting-confirm / awaiting-with-buffer) keeps one operation
  in flight, buffers local edits, and transforms incoming operations before
  applying them — with an echo guard and forced `\n` line endings so client and
  server agree on offsets (D-005).
- Document lifecycle limits (FR8, NFR6): an idle-document reaper drops
  in-memory documents with no connections after a configurable TTL (default
  24 h) and deletes their snapshots, along with orphan snapshot files older
  than the TTL by file mtime, keeping memory and disk bounded. Per-connection
  abuse guards cap message size at 64 KB, throttle operations with a 100 ops/s
  token bucket (a flood closes the connection), and limit each client IP to 10
  concurrently open documents. The TTL and reaper interval are configurable via
  `SYNCPAD_DOC_TTL_SECS` and `SYNCPAD_REAP_SECS`.
- Document snapshots (FR9): dirty documents are written to
  `data/<docId>.json` (content, revision, language, updated-at) every 30 s and
  flushed on graceful shutdown, using an atomic temp-file-plus-rename write.
  Unknown document ids are hydrated lazily from their snapshot on first
  access, so a restart recovers live documents; the replay window is truncated
  at each snapshot to keep memory bounded. The snapshot interval and data
  directory are configurable via `SYNCPAD_SNAPSHOT_SECS` and
  `SYNCPAD_DATA_DIR`.
- Convergence fuzz harness: seeded, reproducible scenarios drive 2–5
  simulated clients (full ot.js-style state machine: one op in flight,
  compose-into-buffer, transform-on-remote) through randomized concurrent
  inserts and deletes — including multibyte characters — with delayed event
  processing, asserting byte-identical convergence between every client and
  the server. Runs in CI on every push; failures name their seed, and the
  workload is tunable via `SYNCPAD_FUZZ_SEEDS`/`SYNCPAD_FUZZ_ROUNDS`.
- Server-side operational transforms (FR3): `op` messages are validated
  against the replay window, transformed across concurrent revision-log
  entries via the pinned `operational-transform` crate, applied, acked to
  the sender, and broadcast to peers; rejected operations never mutate the
  document and force the sender through a `resync` + fresh `init` recovery
  path. Convergence of concurrent and stale-base edits is verified at the
  task level and over real WebSocket connections.
- Per-document tokio tasks that own document state: connections send
  commands over a channel and receive broadcast events, keeping all document
  mutation single-threaded; connections that lag behind the broadcast are
  told to resync.
- Presence: server-assigned adjective-animal names and palette colors,
  join/leave events broadcast to peers, and the current roster delivered in
  `init`, verified end to end over real WebSocket connections.
- Rust workspace with the axum server scaffold: document creation over
  `POST /api/docs` with unguessable 8-character slugs, an in-memory document
  registry, WebSocket connect on `/ws/:docId` that delivers the initial
  document state, and static serving of the built frontend.
- Wire-protocol message types for the sync protocol (`init`, `op`, `ack`,
  `cursor`, `presence`, `language`, `ping`/`pong`, `resync`) with round-trip
  serde tests.
- Vite + React + TypeScript frontend shell: landing page that creates a
  document and navigates to its `/d/:id` URL, a placeholder editor route, and
  a first Vitest unit test.
- CI workflow running the Rust format check, clippy with warnings denied, the
  server test suite, and the frontend build and tests.

## [0.0.1] - 2026-07-11

### Added

- Initial project documentation: README, engineering spec
  (`docs/syncpad-engineering-doc.md`), decision log, and this changelog.
- Contribution, security, and code-of-conduct policies plus issue and PR
  templates.
- Setup and verification scripts for local development.
- Repo hygiene: MIT license, `.gitignore`, `.gitattributes`, `.editorconfig`.
