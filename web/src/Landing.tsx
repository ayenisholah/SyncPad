import { useState } from "react";
import { createDoc } from "./api";

export function Landing() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function onNewDocument() {
    setBusy(true);
    setError(null);
    try {
      const docId = await createDoc();
      window.location.assign(`/d/${docId}`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setBusy(false);
    }
  }

  return (
    <main className="shell">
      <h1 className="wordmark">SyncPad</h1>
      <p className="tagline">
        Real-time collaborative code editing — no accounts, just a link.
      </p>
      <button
        className="new-doc"
        onClick={() => void onNewDocument()}
        disabled={busy}
      >
        {busy ? "Creating…" : "New document"}
      </button>
      {error ? <p className="error">{error}</p> : null}
      <footer className="footer">
        Rust · WebSockets · Operational Transforms ·{" "}
        <a href="https://github.com/ayenisholah/SyncPad">GitHub</a>
      </footer>
    </main>
  );
}
