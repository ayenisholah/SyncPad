# Decision Log (ADR-lite)

Significant technical decisions are recorded here before implementation.
Statuses: Proposed → Approved / Rejected; later possibly Superseded.

## Template

```
## D-XXX: <short title>
- Date: YYYY-MM-DD · Status: Proposed · Decider: Shola Ayeni
- Context: <what prompted this>
- Decision: <what will be done>
- Consequences: <trade-offs, follow-ups>
```

---

## D-001: MIT license

- Date: 2026-07-11 · Status: Approved · Decider: Shola Ayeni
- Context: open-source collaborative-editing project; comparable projects in
  the space are permissively licensed.
- Decision: MIT, © 2026 Shola Ayeni.
- Consequences: maximally permissive; no CLA needed.

## D-002: Operational transforms over CRDTs

- Date: 2026-07-11 · Status: Approved · Decider: Shola Ayeni
- Context: concurrent editing needs a convergence strategy. The two
  established options are OT (a central server transforms concurrent
  operations against each other) and CRDTs (merge-anywhere replicated data
  structures such as Yjs/yrs or diamond-types).
- Decision: OT with an authoritative central server. A server is already in
  the topology to serve the app and broker WebSockets, OT keeps per-document
  memory small, and the revision-log model is straightforward to reason
  about and to test for convergence.
- Consequences: documents are pinned to a single authoritative process;
  horizontal scaling requires document-affinity routing (recorded as future
  work). Offline-first merging is out of scope. A CRDT comparison remains an
  interesting follow-up.

## D-003: Use the `operational-transform` crate, not a hand-rolled OT

- Date: 2026-07-11 · Status: Approved · Decider: Shola Ayeni
- Context: transform/compose/apply correctness (the TP1 property) is the
  hardest part of OT and has a well-tested existing implementation in the
  `operational-transform` crate (a Rust port of ot.js).
- Decision: depend on the crate for the operation algebra; pin its version.
  SyncPad implements the protocol around it: revision ordering, the client
  state machine, cursor transformation, presence, and resync/recovery.
- Consequences: convergence correctness lives in a tested library and is
  additionally guarded by the project's fuzz harness; the client-side state
  machine mirrors the ot.js semantics in TypeScript and is unit-tested
  transition by transition.

## D-006: Single-instance container behind an nginx reverse proxy

- Date: 2026-07-12 · Status: Approved · Decider: Shola Ayeni
- Context: the in-memory + snapshot design pins each document to one process, so
  the deployment is a single stateful-ish instance (horizontal scaling would
  need document-affinity routing — future work). A host to run it on and a way
  to terminate TLS and upgrade WebSockets are needed.
- Decision: ship a multi-stage Docker image (frontend build → server build →
  distroless runtime) run via Docker Compose on a VPS, bound to host loopback,
  with a named volume for `/data`. A host nginx server block reverse-proxies the
  public subdomain to it, forwarding the client IP (`X-Real-IP`) so the
  per-connection limits remain per-user, and passing the WebSocket upgrade. TLS
  is added with certbot; HTTP is used until then. nginx is chosen over the
  spec's Caddy example because the host already runs nginx.
- Consequences: one image, one process, one volume — simple to operate and to
  reason about. The server trusts a forwarded-IP header, which is safe only
  because the container is not publicly bound and is reachable solely through
  the proxy. Multi-region or high-availability deployment is out of scope.

## D-005: Client operation algebra — minimal TypeScript port now

- Date: 2026-07-11 · Status: Approved · Decider: Shola Ayeni
- Context: the browser client runs the ot.js state machine (D-003), which needs
  `apply`/`compose`/`transform` locally to compose buffered edits and transform
  incoming operations. The server's operation algebra lives in the
  `operational-transform` crate; the client needs the same semantics in the
  browser. Options: port the algebra to TypeScript, compile the crate to
  WebAssembly, or depend on the original `ot` npm package (2014, unmaintained).
- Decision: port the crate's operation algebra minimally into
  `web/src/ops.ts` (a `TextOperation` with the same flat-array wire format and
  Unicode-scalar-value counting), kept behind a small module boundary so a
  WebAssembly build of the crate can replace it later without touching the
  state machine. The `ot` package is not used.
- Consequences: one small, dependency-free module carries the client algebra;
  its correctness is guarded by unit tests that mirror the server (a TP1
  property test over randomized concurrent operations, the crate's wire-format
  fixtures, and shared known cases). A future WebAssembly swap would unify the
  algebra on both sides behind the same interface; until then the two
  implementations must be kept in agreement, which the shared tests enforce.

## D-004: Promote futures-util to a runtime dependency

- Date: 2026-07-11 · Status: Approved · Decider: Shola Ayeni
- Context: axum's `WebSocket` is a combined `Stream + Sink`. Forwarding
  document broadcast events to the socket concurrently with reading client
  frames requires splitting it into sink and stream halves, which needs the
  `futures_util::StreamExt::split` combinator. The crate was already a dev
  dependency and is in the dependency graph through axum itself.
- Decision: move `futures-util` from `[dev-dependencies]` to
  `[dependencies]`.
- Consequences: no new crates enter the dependency tree; the runtime
  dependency list grows by one entry that axum already pulled in
  transitively.
