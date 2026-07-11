# Changelog

All notable changes to SyncPad are documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) ·
Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).
Changes accumulate under **[Unreleased]** and roll into a version at each
release.

## [Unreleased]

### Added

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
