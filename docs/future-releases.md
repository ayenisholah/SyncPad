# Future releases

This roadmap records recommended work after the current production-demo
milestone. It is directional rather than a promise: priorities may change as
measurements, incidents, dependency updates, and contributor feedback provide
better evidence. Completed work belongs in [`CHANGELOG.md`](../CHANGELOG.md),
while significant design changes require a proposed decision record in
[`DECISIONS.md`](DECISIONS.md).

## Release principles

- Fix correctness, security, recovery, and operability problems before adding
  product surface area.
- Keep every release reproducible from a tagged container image and retain a
  tested rollback path.
- Add a measurable acceptance test for each performance or reliability claim.
- Preserve the current bearer-link and ephemeral-document model unless an
  approved decision record explicitly changes it.

## v0.1: hardened public preview

The first tagged preview should turn the working demo into a safer and more
predictable service without changing its core architecture.

### Recommended fixes

1. **Reduce public synchronization latency.** Profile the HTTPS/WSS path and
   server/client timing, then establish a repeatable latency budget. The current
   measured public p95 of 1,434 ms is the most visible quality gap.
2. **Self-host browser dependencies.** Bundle Monaco, fonts, and workers with
   the application so editing does not depend on third-party CDNs and can be
   covered by a strict Content Security Policy.
3. **Harden proxy trust.** Accept forwarded client IPs only from configured
   trusted proxies, reject ambiguous forwarding chains, and test direct-access
   and spoofed-header cases.
4. **Bound document resources.** Add explicit maximum document size and
   connection count limits, expose clear client errors when limits are reached,
   and load-test oversized documents and connection churn.
5. **Verify backup recovery.** Automate snapshot-volume backups, document
   retention, and regularly restore into an isolated instance. Record the
   recovery-point and recovery-time observations instead of assuming atomic
   snapshots are sufficient backups.
6. **Make deployment verification immutable.** Deploy a `sha-*` image (or
   digest) rather than a moving tag, record the previous digest automatically,
   and have the smoke test verify the running revision as well as document
   creation.

### Release gate

- CI, convergence fuzzing, and two-browser end-to-end tests pass on the tag.
- Container provenance and SBOM are published and the deployed digest matches
  the release.
- Backup restore and rollback are exercised successfully.
- The public latency measurement improves against the documented baseline or
  the remaining bottleneck and accepted threshold are documented.
- High-severity dependency and container findings are resolved or explicitly
  risk-accepted.

## v0.2: resilience and editor quality

After the hardened preview is stable, improve recovery and daily editing:

- Add reconnect backoff with jitter, offline/read-only status, and tests for
  network flapping, server restarts, and long-lag resynchronization.
- Add collaborative undo/redo with protocol-level convergence tests before
  exposing it in the UI.
- Improve keyboard, screen-reader, focus, contrast, and responsive-layout
  support; add automated accessibility checks plus manual keyboard testing.
- Add structured logs and metrics for active documents, connections, rejected
  operations, resyncs, snapshot failures, and latency, with actionable alerts.
- Add version history or explicit export/import if user research shows that
  the current ephemeral snapshot model causes accidental data loss.
- Test more browsers and mobile viewports while keeping Chromium convergence
  coverage as the minimum release gate.

## Later: scale and optional product capabilities

These items materially change the architecture or product model and should not
be folded into a routine feature release:

- Horizontal scaling requires document-affinity routing plus shared durable
  state, or a deliberate move from centralized OT to another replication
  model. Benchmark and document the choice in an ADR first.
- Accounts, access control, private documents, and encryption at rest would
  replace the current bearer-link trust model and require threat modeling,
  migration, deletion, and key-management designs.
- Offline-first editing requires durable client state and merge semantics that
  the current server-authoritative protocol intentionally does not provide.
- Multi-region availability should follow observed demand and a defined
  availability objective, not precede them.

## v1.0 readiness

Consider 1.0 only when the supported use case and compatibility policy are
explicit, protocol and snapshot migrations are tested, monitoring and incident
runbooks are operational, backup/restore is routinely exercised, and the
security and performance limits are supported by current measurements. Until
then, releases should remain clearly labeled pre-1.0.
