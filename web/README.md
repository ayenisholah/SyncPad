# Web frontend

The frontend is Vite, React, TypeScript, and Monaco. `src/App.tsx` routes the
landing page and `/d/:id`; `Editor.tsx` integrates Monaco, connection state,
presence, cursors, language, and status UI. `connection.ts`, `otClient.ts`, and
`ops.ts` implement transport and client OT. Unit tests live beside source;
Playwright suites are in `e2e/`.

During `npm run dev`, Vite proxies `/api` and `/ws` to the Rust server at
`127.0.0.1:8090`. Start `cargo run -p syncpad-server` from the repository root
first. Production serves `web/dist` from the same origin. Node 22+ and npm are
required. Monaco workers and Google Fonts are currently fetched from external
networks, so first load and browser tests require network access.

## Commands

| Script | Purpose |
|---|---|
| `dev` | Start the Vite development server. |
| `build` | Type-check and build the production bundle. |
| `preview` | Preview the built bundle. |
| `test`, `test:unit` | Run Vitest once. |
| `e2e`, `test:e2e` | Build, start the Rust server, and run Playwright. |
| `stress` | Bundle and run the WebSocket load harness. |
| `measure` | Alias for the stress harness; pass its flags after `--`. |
| `gen:assets` | Regenerate favicon/social assets. |
| `docs:architecture` | Render Mermaid in `docs/architecture.md` to PNG. |
| `docs:demo` | Capture the deterministic two-browser README demo. |
| `docs:assets` | Generate architecture and demo media. |
| `docs:check` | Check local Markdown links, images, scripts, and README sections. |
| `check` | Build, unit-test, and check documentation. |

Use `npm run measure -- --help` for load options. Production measurement is a
manual workflow; do not aim the harness at systems without permission.

## Testing and troubleshooting

Run `npm ci`, then `npm run check`. Playwright additionally needs Chromium
(`npx playwright install chromium`) and a working Rust toolchain. E2E uses port
8090 and disposable `target/e2e-data`; stop an existing server if startup
fails. If Monaco stays on “Connecting”, confirm the Rust server is running and
the browser can reach `/ws`. Corporate content blockers can prevent Monaco or
fonts loading. Generated documentation capture uses disposable local documents
only and never reads the production service.
