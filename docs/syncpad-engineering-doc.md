# SyncPad — Engineering Document

**Real-time collaborative code editor — Rust, WebSockets & operational transforms**

| | |
|---|---|
| **Status** | Draft v1.0 |
| **Created** | 2026-07-10 |
| **Target duration** | 2 weeks (MVP shippable at end of week 1) |
| **Repo** | Rust server + Vite/React/Monaco frontend |

---

## Table of contents

1. [Concept & problem statement](#1-concept--problem-statement)
2. [Goals and non-goals](#2-goals-and-non-goals)
3. [Users and use cases](#3-users-and-use-cases)
4. [Requirements](#4-requirements)
5. [System architecture](#5-system-architecture)
6. [Detailed design](#6-detailed-design)
7. [Technology decisions](#7-technology-decisions)
8. [UI/UX design](#8-uiux-design)
9. [Project plan & milestones](#9-project-plan--milestones)
10. [MVP definition](#10-mvp-definition)
11. [Testing & verification strategy](#11-testing--verification-strategy)
12. [Deployment & operations](#12-deployment--operations)
13. [Risks & mitigations](#13-risks--mitigations)
14. [Definition of done & acceptance tests](#14-definition-of-done--acceptance-tests)
15. [Future work](#15-future-work)
16. [Appendix](#16-appendix)

---

## 1. Concept & problem statement

### The idea in one sentence

A Google-Docs-style collaborative **code** editor: anyone with the link edits the same document simultaneously in Monaco, a Rust server merges concurrent edits with **operational transforms (OT)** so no keystrokes are ever lost, and no database is involved — documents live in memory with periodic snapshots.

### The problem

Concurrent editing is a consistency problem: two people type at position 10 at the same instant, and a naive "last write wins" server silently destroys one of the edits. The two established solutions are **OT** (a central server transforms concurrent operations against each other so they compose — the Google Docs approach) and **CRDTs** (merge-anywhere data structures — the Figma/Yjs approach). OT with a central server is the simpler, lower-memory choice when a server is already in the topology, and it produces an architecture that is easy to reason about and easy to demonstrate.

SyncPad implements the OT-server approach in Rust: one authoritative document per room, a revision log, server-side transformation of concurrent ops, and broadcast to all connected editors — with live cursors and presence so the collaboration is visible.

### Why this design

- The system is instantly demonstrable: open two windows, type in both, and watch them converge — the correctness property is directly visible.
- OT has a well-defined correct answer; depending on a tested library for the transform algebra instead of hand-rolling it is a deliberate engineering decision (see §7 and D-003).
- The no-database design (in-memory + snapshots) is a deliberate architecture decision with a clear state lifecycle: expiry, crash recovery, and bounded memory.
- Room-per-task concurrency on tokio keeps all mutation single-threaded per document, with no locks around OT state.

---

## 2. Goals and non-goals

### Goals (binding)

| # | Goal |
|---|---|
| G1 | Create/join a document via shareable URL (`/d/:id`, random slug) |
| G2 | Real-time sync over WebSockets using OT — via the `operational-transform` crate, **not** hand-rolled |
| G3 | Monaco editor with syntax highlighting + language picker |
| G4 | Live cursors: other users' cursors and selections with names/colors |
| G5 | Presence list (who's in the document) |
| G6 | In-memory document store, auto-expire after 24 h idle |
| G7 | Periodic snapshot to disk (JSON file) so a restart doesn't wipe live docs |
| G8 | Deployed publicly; two browsers stay consistent under fast typing; measured op→apply latency |

### Non-goals (cut ruthlessly — also binding)

| # | Non-goal | Why it's cut |
|---|---|---|
| N1 | Accounts, auth, permissions | Anyone with the link edits — this is the product decision, state it plainly in the README |
| N2 | Database | The in-memory + snapshot design *is* a feature of the architecture |
| N3 | Document history / versioning UI | The revision log exists internally for OT; exposing it is future work |
| N4 | Rich text | Plain code text only — OT on plain text is well-defined; rich text OT is a research project |
| N5 | Mobile layout polish | Desktop-first; Monaco is desktop-first anyway |

> **Scope rule:** anything not in the Goals table goes to [§15 Future work](#15-future-work) and does not get built.

---

## 3. Users and use cases

### Primary use

SyncPad is built demo-first: the target experience is two side-by-side windows converging under deliberately hostile typing. The users are pairs who share a link to sketch code together — interviews, teaching, quick collaborative debugging — and anyone evaluating the sync engine by opening a second window. Optimize for the two-window scenario being flawless.

### User stories

| ID | Story | Priority |
|---|---|---|
| US1 | As a visitor, I click "New document" and land on `/d/x7k2p9q1` ready to type | Must |
| US2 | As a collaborator, I open a shared link and see the current document content within 1 s | Must |
| US3 | As two people typing in the same line simultaneously, we both keep our edits — the document converges identically on both screens | Must |
| US4 | As a collaborator, I see the other person's named, colored cursor and selection move live | Must |
| US5 | As a collaborator, I see who's in the document (presence list) and people disappear when they leave | Must |
| US6 | As a user, I pick a language (TS, Rust, Python, …) and syntax highlighting updates for everyone | Should |
| US7 | As a user whose connection blips, I reconnect and my offline keystrokes merge instead of vanishing | Should |
| US8 | As the operator, a server restart brings documents back from the last snapshot | Must |

### Demo script (design target)

The 3-minute demo: click "New document" → copy link into a second window side-by-side → type simultaneously in both, same line — text converges, nothing lost → highlight a block in window A, window B shows the colored selection with a name tag → switch language to Rust, both re-highlight → kill the server, restart it, reload — the document is still there (snapshot) → point at the latency readout in the status bar (~X ms op→apply).

---

## 4. Requirements

### 4.1 Functional requirements

| ID | Requirement | Priority | Acceptance criterion |
|---|---|---|---|
| FR1 | `POST /api/docs` creates a doc, returns `{docId}` (random 8-char slug); `GET /d/:id` serves the editor | Must | curl + browser flow works; unknown slug → friendly "create it?" page |
| FR2 | WS `/ws/:docId`: on connect, server sends `init {revision, content, language, participants}` | Must | Late joiner sees exact current content |
| FR3 | Client sends `op {baseRevision, ops}`; server transforms against concurrent ops since `baseRevision`, applies, appends to revision log, broadcasts `op {revision, ops, authorId}` to others and `ack {revision}` to the sender | Must | Convergence fuzz test (§11) passes; two-window hostile-typing scenario converges |
| FR4 | Client implements the standard OT client state machine (Synchronized / AwaitingConfirm / AwaitingWithBuffer): one in-flight op, local composition while waiting | Must | Fast typing never floods the server; ordering preserved |
| FR5 | Cursor/selection messages (throttled ~50 ms) broadcast with authorId; server transforms stored cursor positions when ops apply so remote cursors stay accurate | Must | Remote cursor lands on the right character even during concurrent edits |
| FR6 | Presence: join/leave events; server assigns name (adjective-animal) + color from a fixed palette | Must | Presence list matches reality within 1 s |
| FR7 | Language picker: `setLanguage` message broadcast; part of doc state and snapshot | Should | Both windows re-highlight |
| FR8 | Doc lifecycle: in-memory `DashMap`; reaper expires docs idle > 24 h; expired doc slug → create-new page | Must | Manual clock test / shortened-TTL test passes |
| FR9 | Snapshots: dirty docs written to `data/<docId>.json` (content, revision, language, updatedAt) every 30 s and on graceful shutdown; lazily loaded on first access after restart | Must | Kill -9 loses ≤ 30 s; graceful restart loses nothing |
| FR10 | Reconnect: client reconnects with `{docId, lastKnownRevision}`; server replays missed ops from the revision log (log truncated below the snapshot — full resync fallback) | Should | Network-blip test: offline keystrokes merge on reconnect |
| FR11 | Latency instrumentation: ops carry client send timestamp; receiving clients log op→apply delta; status bar shows rolling p50 | Must | The published latency number comes from this instrumentation |

### 4.2 Non-functional requirements

| ID | Requirement | Target | How measured |
|---|---|---|---|
| NFR1 | Op→remote-apply latency (2 clients, deployed server) | p50 < 50 ms target — **publish only the measured value** | FR11 instrumentation |
| NFR2 | Correctness | Zero divergence: N clients end byte-identical after any interleaving | Fuzz harness (§11) + manual abuse |
| NFR3 | Concurrency | Hundreds of concurrent sessions target on a small VPS — **publish only the measured number** from the stress test | Headless-client stress script |
| NFR4 | Doc size | Sane behavior to 500 KB text; reject ops on docs > 1 MB with a clear error | Manual paste test |
| NFR5 | Memory | Bounded: revision log truncated below last snapshot; expired docs freed | Soak + heap observation |
| NFR6 | Security | Slug is the only capability — 8 chars from a 32-char alphabet (~10^12); no doc enumeration endpoint; payload/op-rate caps per connection | Review |
| NFR7 | Availability | Restart-safe via snapshots; panics isolated per doc task | Kill/restart test |

---

## 5. System architecture

### 5.1 Topology

```
Browser A ── React + Monaco ──┐
  │ OT client state machine   │ ops / cursors / presence (JSON over WS)
  ▼                           ▼
┌──────────────────────────────────────────────┐
│ Rust server (axum + tokio)                   │
│  ├─ HTTP: POST /api/docs · GET /d/:id ·      │
│  │        static frontend                    │
│  ├─ WS: /ws/:docId ─► per-doc room task      │
│  │       ┌────────────────────────────────┐  │
│  │       │ Doc task (one per live doc)    │  │
│  │       │  content: Rope/String          │  │
│  │       │  revision log: Vec<Operation>  │  │
│  │       │  OT: operational-transform     │  │
│  │       │  presence + cursor map         │  │
│  │       │  broadcast to subscribers      │  │
│  │       └────────────────────────────────┘  │
│  ├─ DashMap<DocId, DocHandle>                │
│  ├─ snapshot task (30s, dirty docs → JSON)   │
│  └─ reaper task (24h idle expiry)            │
└──────────────────────────────────────────────┘
                    │
              data/*.json  (snapshots — the only persistence)
```

### 5.2 Components

| Component | Responsibility | Tech |
|---|---|---|
| **Doc task** | Owns one document: applies/transforms ops, orders revisions, transforms cursors, broadcasts | tokio task + `mpsc` in / `broadcast` out |
| **Registry** | docId → handle; create-on-demand; lazy snapshot load | `DashMap<DocId, DocHandle>` |
| **OT engine** | `transform`, `compose`, `apply` for text operations | `operational-transform` crate |
| **Snapshot service** | Periodic dirty-doc persistence + shutdown flush | tokio interval task, `serde_json`, atomic write (tmp + rename) |
| **Reaper** | Expire idle docs, drop handles, delete stale snapshots | tokio interval task |
| **Frontend** | Monaco editing, OT client state machine, remote cursors (Monaco decorations), presence UI | Vite + React + `@monaco-editor/react` + `ot.js`-style TS client |

### 5.3 Data flow (one keystroke, two clients)

1. A types `x`. Monaco fires `onDidChangeModelContent`; the change converts to a text operation (`retain 10, insert "x", retain 4`).
2. A's OT client: if Synchronized → send `op {baseRevision: 12, ops}` and enter AwaitingConfirm; if already awaiting → compose into the buffer.
3. Doc task receives it. Concurrent ops 13..14 exist → transform A's op across them, apply to content, append as revision 15.
4. Server sends `ack {revision: 15}` to A (A leaves AwaitingConfirm or promotes its buffer) and `op {revision: 15, ops', authorId: A}` to B.
5. B's OT client transforms the incoming op against its own pending op/buffer if any, then applies to Monaco (guarding the change listener against echo).
6. Stored cursor positions are transformed by the same op; cursor updates broadcast; B's latency logger records `now − sentAt`.

---

## 6. Detailed design

### 6.1 OT integration (the core — use the crate, own the protocol)

The `operational-transform` crate (Rust port of ot.js) provides `OperationSeq` with `apply`, `compose`, and `transform` satisfying the TP1 property: for concurrent `a`, `b`: `apply(apply(S,a), b') == apply(apply(S,b), a')` where `(a', b') = transform(a, b)`. **Do not hand-roll any of this** (binding — see D-003). What SyncPad owns is the *protocol around it*:

**Server per doc:**

```rust
struct Doc {
    content: String,               // consider ropey if large-doc perf matters
    revision: u64,
    log: Vec<OperationSeq>,        // ops since last snapshot (replay window)
    presence: HashMap<UserId, Presence>,   // name, color, cursor, selection
    dirty: bool,
    last_active: Instant,
}
```

On `op {baseRevision, ops}`:
1. Reject if `baseRevision > revision` (client from the future = bug) or below the replay window (force full resync via `init`).
2. `for concurrent in &log[baseRevision..]: ops = ops.transform(concurrent)?.0`
3. `content = ops.apply(&content)?` — an apply error means a broken client: disconnect it with a resync hint, never corrupt the doc.
4. Push to log, bump revision, transform all stored cursors through `ops`, mark dirty, broadcast + ack.

**Client state machine** (the ot.js pattern, ~150 lines of TS — this is protocol, not transform math, so writing it is fine and instructive):

- `Synchronized` —local edit→ send, go `AwaitingConfirm(sent)`
- `AwaitingConfirm` —local edit→ `AwaitingWithBuffer(sent, buffer)`; further edits compose into `buffer`
- ack → promote: buffer (if any) is sent next; server op → transform `sent`/`buffer` against it, apply the transformed op to the editor

One op in flight at a time keeps the server's transform window small and makes ordering trivial.

### 6.2 WebSocket message protocol (JSON)

**Client → Server**

| Type | Fields | Notes |
|---|---|---|
| `op` | `baseRevision, ops, sentAt` | `ops` in the crate's JSON form (`["retain",n]`-style primitives) |
| `cursor` | `position, selection?` | throttled to 50 ms client-side |
| `setLanguage` | `language` | from an allowlist |
| `ping` | `t0` | keepalive + clock reference for latency logging |

**Server → Client**

| Type | Fields | Notes |
|---|---|---|
| `init` | `revision, content, language, participants, selfId` | on connect and on forced resync |
| `op` | `revision, ops, authorId, sentAt` | already transformed to tip |
| `ack` | `revision` | sender's op accepted at this revision |
| `cursor` | `authorId, position, selection?` | positions valid at current revision |
| `presence` | `joined?/left?, participant(s)` | roster deltas |
| `language` | `language` | |
| `pong` | `t0, t1` | |
| `resync` | — | client must drop state and await `init` |

### 6.3 Monaco integration (the fiddly 20 %)

- **Echo guard**: applying a remote op calls `model.applyEdits`; an `applyingRemote` flag makes the change listener ignore those events (the classic infinite-loop bug).
- **Offsets, not line/column**: OT ops speak absolute character offsets; convert with `model.getOffsetAt`/`getPositionAt`. Beware line endings — force `\n` (`model.setEOL`) so server and client agree on offsets.
- **Undo**: remote edits applied via `applyEdits` on the model keep the local undo stack usable; perfect collaborative undo is out of scope (N3) — document the limitation.
- **Remote cursors**: Monaco `deltaDecorations` — a 2 px colored bar (`className` per color) + name label via CSS `::after`; selections as semi-transparent range decorations. Cache decoration ids per user; remove on leave.
- **Composition/IME**: buffer ops during `compositionstart`→`compositionend` to avoid transforming half-composed input.

### 6.4 Document lifecycle & persistence

- **Create**: `POST /api/docs` inserts an empty doc handle; slug from a 32-char unambiguous alphabet, 8 chars.
- **Lazy load**: WS connect for an unknown docId first checks `data/<id>.json`; found → hydrate (revision from snapshot, empty log), else treat as new.
- **Snapshot**: every 30 s, docs with `dirty` write `{content, revision, language, updatedAt}` atomically (write `.tmp`, `rename`). On graceful shutdown (SIGTERM), flush all dirty docs before exit. Log truncates to the snapshot revision — reconnects below that get `resync`.
- **Expiry**: reaper drops in-memory docs idle > 24 h and deletes snapshots idle > 24 h (snapshot mtime). Expired link → create-new page with a friendly note.

### 6.5 Latency measurement

- Ops carry `sentAt` (sender clock). Receivers log `applyAt − sentAt`; a rolling p50 shows in the status bar.
- Cross-machine clock skew: NTP-style ping/pong offset estimation (server echoes `t0, t1`; keep the min-RTT sample). For the headline number, also do the honest same-machine two-window measurement.
- Server additionally records queue→broadcast time per op (its own share of the budget) in logs — useful for the README breakdown: client→server hop + transform + fan-out + server→client hop.

### 6.6 Server internals

- One tokio task per doc; all mutation single-threaded per doc (no locks around OT state). Inbound via `mpsc`, outbound via `tokio::sync::broadcast` (each WS connection's writer subscribes; lagging receivers get disconnected with `resync` — bounded memory).
- Per-connection limits: 100 ops/s token bucket, 64 KB max message, 10 docs per IP concurrently (light abuse guard, NFR6).
- Panic isolation: doc-task panic drops that doc (clients get close + reconnect → snapshot hydrate); server survives.

---

## 7. Technology decisions

| Decision | Choice | Alternatives considered | Rationale |
|---|---|---|---|
| Consistency algorithm | **OT (central server)** | CRDT (yrs/Yjs, diamond-types) | Server is already authoritative; OT is smaller in memory, and the architecture is easier to reason about and explain. The CRDT tradeoff is documented in the README (D-002) |
| OT implementation | `operational-transform` crate | Hand-rolled | Do NOT write OT from scratch (D-003). Correctness lives in a tested library; the protocol around it is where this project's engineering goes |
| Web framework | **axum** | warp | tokio/tower ecosystem, widely used, well documented |
| Doc text storage | `String` first, `ropey` if profiling demands | rope from day 1 | Code files are small; don't pre-optimize. The swap is isolated inside the doc task |
| Editor | Monaco (`@monaco-editor/react`) | CodeMirror 6 | VS Code familiarity; first-class TypeScript support; the decoration API covers remote cursors well |
| Client OT | ~150-line TS state machine (ot.js pattern) | depend on ot.js (2014, unmaintained) | The state machine is small, well-documented, and writing it in TS keeps deps fresh; transform math still comes from the server crate's semantics (client only composes/transforms via the same op algebra, ported minimally) |
| WASM for client OT | **Deferred** (stretch goal) | wasm-bindgen build of the crate | Start with the TS client; the wasm build is a Week-2 stretch if time remains — one op algebra on both sides |
| Snapshot format | JSON file per doc | SQLite, sled | N2 says no database; per-doc files make the persistence story legible (`ls data/`) |
| Frontend | Vite + React | vanilla | Presence list/status UI + Monaco wrapper are natural React |

---

## 8. UI/UX design

### 8.1 Screens

1. **Landing** — wordmark, one "New document" button, one-line pitch, link to GitHub.
2. **Editor** (`/d/:id`) — Monaco filling the viewport; top bar: doc slug + copy-link button, language picker, presence avatars (colored dots + names on hover); status bar: connection state, rolling op→apply latency (`sync 23 ms`), revision number. Remote cursors/selections rendered in-editor.

Design principles: the editor is the app — chrome stays minimal; presence colors match cursor colors exactly (that mapping *is* the UX); the latency readout is a feature, not debug info — it is the product's headline number rendered live.

Visual language: VS Code-dark background (`#1e1e1e`), violet accent (`#a78bfa`), JetBrains Mono for code and the status bar, Inter for UI chrome.

### 8.2 Design acceptance

- [ ] Editor design iterated until remote cursors + name tags read clearly at screen-capture compression
- [ ] Architecture diagram exported to `docs/architecture.png`, embedded in README
- [ ] Social preview image set for the repository

---

## 9. Project plan & milestones

2 weeks, ~15–20 focused hours/week. Server correctness first — the sync engine is testable with `websocat`/scripts before any UI exists.

### Week 1 — server: rooms, OT, lifecycle *(→ MVP core)*

| Day | Work |
|---|---|
| 1 | Repo scaffold (cargo + Vite), axum server, `POST /api/docs`, slug gen, static serving, WS upgrade + `init` echo |
| 2 | Doc task + registry: mpsc/broadcast wiring, presence join/leave, JSON protocol codecs (with tests) |
| 3 | OT integration: transform-against-log, apply, ack/broadcast, resync path; drive with two scripted WS clients |
| 4 | **Convergence fuzz harness** (§11) — run it until it's boringly green; fix ordering bugs now, not during a live demo |
| 5 | Snapshots (atomic write + shutdown flush + lazy hydrate), reaper, per-connection limits. **Checkpoint: two scripted clients converge under randomized concurrent load, restart-safe** |

### Week 2 — frontend, cursors, deploy, publish *(→ MVP complete then polish)*

| Day | Work |
|---|---|
| 1 | React shell + Monaco; OT client state machine in TS with unit tests; echo guard; offset mapping |
| 2 | End-to-end typing between two browsers; ack/transform paths exercised; IME guard. **Checkpoint = MVP (§10)** |
| 3 | Remote cursors + selections (decorations, server-side cursor transform), presence bar, language picker, status-bar latency readout |
| 4 | Deploy (Fly.io or VPS + Caddy), Dockerfile (`docker run` one-liner), stress script (N headless clients — record the real session count), measure op→apply latency |
| 5 | README (screen capture → architecture → OT explainer → quickstart), record measured metrics, publish. Stretch (only if done): wasm-bindgen client transform build |

### Milestone gates

| Gate | Criterion | If missed |
|---|---|---|
| M1 (W1D4) | Fuzz harness green — zero divergence | Stop everything; a diverging editor is worthless no matter how pretty |
| M2 (W2D2) | **MVP**: two browsers, Monaco, converging edits, deployed-ready | Cut cursors/language to post-MVP polish; typing convergence cannot slip |
| M3 (end W2) | Deployed + measured + README complete | Cut the wasm stretch and reconnect-replay (resync-only is acceptable); documented claims match what shipped |

---

## 10. MVP definition

**MVP = the smallest thing that proves the thesis:** two browsers editing one document that provably never diverges.

**In the MVP (Week 2, day 2):**
- Create/join via slug URL
- Monaco editing with OT sync, ack/transform client state machine
- Presence join/leave (list, no cursors yet)
- In-memory docs + snapshots + expiry
- Fuzz-verified convergence

**Explicitly *not* in the MVP (days 3–5):** live cursors/selections, language picker, latency status bar, reconnect replay, deployment, measured numbers.

**Blessed degraded MVP** (if Week 1 slips): plain `<textarea>` instead of Monaco with the same OT protocol — the sync engine is the product; swap the editor in later.

---

## 11. Testing & verification strategy

| Layer | What | How |
|---|---|---|
| Unit (Rust) | Protocol codecs, slug gen, snapshot round-trip, cursor transform, replay-window edges (below-window → resync) | `cargo test` |
| **Convergence fuzz** | N=2..5 scripted clients apply random ops (insert/delete at random offsets, random delays, interleaved acks) for 10k rounds; assert all clients + server end byte-identical; shrink & log failing seeds | Rust integration test binary — **the most important test in the repo**; run in CI on every push |
| Unit (TS) | OT client state machine: all transitions, compose-into-buffer, transform-on-remote; offset↔position mapping | Vitest |
| E2E | 2 Playwright browsers: simultaneous typing at the same offset → identical final text; late-join sees content; presence updates; cursor decoration appears at the right position | Playwright against a spawned server |
| Persistence | SIGTERM → restart → content intact; kill -9 → ≤30 s loss; expired doc → create page | Scripted with short TTLs |
| Stress | Headless-client script: ramp sessions (2 clients/doc, steady typing) until latency degrades; **record the measured number** | Manual on the deployed box, written to `docs/measurements.md` |
| Latency | FR11 instrumentation, same-machine two-window baseline + cross-machine with offset estimation | Manual, methodology in README |

CI: `cargo test` (incl. fuzz with fixed seed count) + `cargo clippy -- -D warnings` + Vitest + Playwright headless.

---

## 12. Deployment & operations

- **Host**: Fly.io (simplest for a single stateful-ish container: volume for `data/`) or a small VPS + Caddy for TLS. Single instance **by design** — in-memory docs pin a doc to a process; horizontal scaling needs doc-affinity routing (documented as future work).
- **Container**: multi-stage Dockerfile (cargo build → distroless), frontend built into the binary's static dir; `docker run -p 8080:8080 -v syncpad-data:/data ghcr.io/ayenisholah/syncpad` is the README one-liner.
- **Shutdown**: SIGTERM → flush dirty snapshots → close WS with a reconnect code.
- **Logs**: `tracing` JSON; per-op server-side timing at debug level.
- **Backups**: `data/` is disposable-by-policy (24 h docs) — say so; the volume just survives restarts.

---

## 13. Risks & mitigations

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Divergence bugs (ordering, transform misuse, echo loops) | High | Fatal to the product | Fuzz harness at W1D4 gate, before any UI; one-op-in-flight protocol keeps the window small |
| R2 | Monaco offset/EOL/IME edge cases corrupt ops | Medium | High | Force `\n` EOL; offset-mapping unit tests; IME buffering; Playwright same-offset test |
| R3 | Client OT state machine subtly wrong | Medium | High | It's ~150 lines — full-transition unit tests; mirror the well-documented ot.js semantics exactly |
| R4 | `operational-transform` crate gaps (unmaintained corners) | Low | Medium | It's a faithful ot.js port with tests; pin the version; the fuzz harness would surface issues immediately |
| R5 | wasm stretch goal eats real deliverables | Medium | Medium | It's gated to W2D5 leftover time only; documented claims match what shipped |
| R6 | Abuse of the open demo (no auth by design) | Low | Low | Doc/IP caps, op rate limits, 1 MB doc cap, 24 h expiry |
| R7 | 2-week estimate slips | Medium | Medium | Gates with pre-decided cuts; textarea degraded MVP pre-authorized |

---

## 14. Definition of done & acceptance tests

Ship when **all** pass:

1. **Public URL**: two browsers (ideally two machines) edit simultaneously; deliberately hostile same-line typing converges to identical text every time.
2. **Fuzz green**: convergence harness passes in CI with zero divergence.
3. **Cursors + presence**: named colored cursors and selections track correctly during concurrent edits; roster accurate.
4. **Lifecycle**: graceful restart loses nothing; kill -9 loses ≤ 30 s; 24 h expiry works (short-TTL test).
5. **Measured latency** recorded with methodology and published in the README — never an estimated number.
6. **Stress number** recorded the same way — concurrent-session capacity is published only as measured.
7. **README**: demo capture at top → architecture diagram → "how OT works here" explainer (with the deliberate crate-not-hand-rolled note and the OT-vs-CRDT tradeoff) → `docker run` one-liner.

---

## 15. Future work (explicitly deferred)

- Document history / time-travel UI on top of the revision log
- Collaborative undo (transform-aware undo stacks)
- WASM client transform (the deferred stretch) — one op algebra on both sides
- Horizontal scaling: doc-affinity routing (consistent hashing on docId) across nodes
- Read-only share links; optional edit tokens
- CRDT branch (yrs) as a comparison writeup
- Rich text / markdown preview mode

---

## 16. Appendix

### 16.1 Planned repo layout

```
syncpad/
├── Cargo.toml
├── server/
│   └── src/
│       ├── main.rs            # axum app, routes, static, shutdown
│       ├── registry.rs        # DashMap, lazy hydrate, create
│       ├── doc.rs             # doc task: OT apply/transform, presence, broadcast
│       ├── protocol.rs        # message types + serde (unit-tested)
│       ├── snapshot.rs        # atomic JSON writes, flush, reaper
│       └── limits.rs          # token buckets, size caps
├── web/
│   └── src/
│       ├── main.tsx           # landing + editor routes
│       ├── otClient.ts        # the state machine (unit-tested)
│       ├── ops.ts             # op algebra helpers, offset mapping
│       ├── connection.ts      # WS + reconnect + latency logging
│       ├── cursors.ts         # Monaco decorations for remote users
│       └── ui/                # top bar, presence, status bar
├── tests/
│   ├── fuzz_convergence.rs    # THE test
│   └── e2e/                   # Playwright
├── scripts/stress.ts          # headless session ramp
├── deploy/                    # Dockerfile, fly.toml / Caddyfile
└── docs/                      # this doc, architecture.png, measurements.md
```

### 16.2 Key crate/package list

**Rust:** `axum`, `tokio`, `operational-transform`, `dashmap`, `serde`/`serde_json`, `rand`, `tracing`, `tower-http`.
**Web:** `vite`, `react`, `@monaco-editor/react`, `typescript`, `vitest`, `playwright`.

### 16.3 Glossary

| Term | Meaning |
|---|---|
| **OT (operational transform)** | Technique where concurrent edit operations are transformed against each other so applying them in either order converges |
| **TP1** | The transformation property guaranteeing two-party convergence; provided by the crate |
| **Revision** | Monotonic counter of accepted ops; clients base their ops on a revision |
| **AwaitingConfirm / AwaitingWithBuffer** | Client states: one op in flight; local edits compose into a buffer meanwhile |
| **Resync** | Recovery path: client drops state and re-initializes from the server's current content |
| **Snapshot** | Periodic JSON persistence of a doc; the only durability (no database, by design) |
| **CRDT** | Conflict-free replicated data type — the decentralized alternative to OT (deliberately not used; see §7) |
