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

## D-007: Continuous delivery via GitHub Actions to GHCR and the VPS

- Date: 2026-07-12 · Status: Approved · Decider: Shola Ayeni
- Context: the single-instance deployment (D-006) needs a repeatable way to build
  the image and roll the container without hand-building on the host. The source
  lives on GitHub, so its Actions and Container Registry are already available.
- Decision: two GitHub Actions workflows. **Container Image** builds the
  multi-stage image and pushes it to the GitHub Container Registry
  (`ghcr.io/ayenisholah/syncpad`), authenticated with the built-in job token (no
  personal access token), on every push to `main` (tag `edge`), on `v*` tags,
  and on demand; images also carry the git short SHA and release version.
  **Deploy Production** is a manual `workflow_dispatch` (an `image_tag` input) —
  nothing deploys on push. It runs against a GitHub **Environment** (`production`)
  whose secrets hold the VPS host, user, a dedicated SSH deploy key, and the
  pinned host key; it ships the `deploy/` bundle over scp and runs `docker
  compose pull` + `up -d` + a create-doc smoke check. Compose selects the image
  through `SYNCPAD_IMAGE`; the registry package is public so the host pulls
  without a login.
- Consequences: deploying is one click on a chosen tag, and rollback is the same
  click on an older tag. Deploy credentials live only in the `production`
  environment's encrypted secrets, which pull-request runs (including from forks)
  cannot read, and deploy is manual-only so no push or fork can ship. Host-key
  pinning (`StrictHostKeyChecking=yes`) and a purpose-scoped, independently
  revocable deploy key keep the SSH path tight. The VPS needs no repository
  clone. This refines D-006; the runtime topology (one instance behind nginx) is
  unchanged.

## D-008: Client-side social sharing of code samples

- Date: 2026-07-12 · Status: Approved · Decider: Shola Ayeni
- Context: users want to share a code sample from a document to social media.
  A server-rendered per-document preview would expose document content to
  crawlers, add server-side image rendering, and break when a document expires
  after 24 h idle — at odds with the unguessable-link privacy model.
- Decision: sharing is entirely client-side. The editor renders the current
  selection (or the whole document) into a branded code image the user can
  download or copy, alongside share-intent links (X, LinkedIn, Reddit) that
  carry the document URL. No server changes and no per-document Open Graph. The
  snippet is syntax-highlighted by reusing Monaco's `editor.colorize` (no new
  highlighter); rasterization uses the `html-to-image` library.
- Consequences: documents are never crawled and sharing survives expiry (the
  image is self-contained; the link is just a link). One new runtime dependency
  (`html-to-image`). The shared image reflects a point-in-time snapshot, not a
  live view — which is the intent.

## D-009: Site metadata, branding, and SEO assets

- Date: 2026-07-12 · Status: Approved · Decider: Shola Ayeni
- Context: the deployed site had only a `<title>` — no favicon, social preview,
  or search metadata. The owner's sibling site sets the house style to match.
- Decision: add a full metadata head (description, canonical, theme-color,
  Open Graph + Twitter `summary_large_image`, a favicon/apple-touch/manifest
  set, and a JSON-LD `SoftwareApplication` block) with SyncPad's own violet
  theme and logo. Document routes (`/d/:id`) are kept out of search: `robots.txt`
  disallows `/d/`, `sitemap.xml` lists only the landing page, and doc routes set
  a runtime `noindex`. Raster assets (favicons, the 1200×630 Open Graph image)
  are generated from checked-in SVG sources by a dev-only script (`sharp`,
  `png-to-ico`); the generated files are committed so the production build needs
  no native image tooling.
- Consequences: rich link previews and correct favicons without exposing
  ephemeral documents to search. Two new devDependencies used only for asset
  regeneration, kept out of the runtime and CI build. SVG remains the source of
  truth for the brand marks.

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
