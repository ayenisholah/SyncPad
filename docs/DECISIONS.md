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
