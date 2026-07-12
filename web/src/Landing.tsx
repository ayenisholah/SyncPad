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
      <span className="badge">RUST · WEBSOCKETS · OT</span>
      <img className="hero-mark" src="/favicon.svg" alt="" width={88} height={88} />
      <h1 className="wordmark">SyncPad</h1>
      <p className="tagline">
        Real-time collaborative code editing. Share a link and edit the same
        document together in your browser — no accounts, no setup.
      </p>
      <button
        className="new-doc"
        onClick={() => void onNewDocument()}
        disabled={busy}
      >
        {busy ? "Creating…" : "New document"}
      </button>
      <div className="values">
        <span className="value">
          <i className="dot" /> <b>Real-time</b> <span>sub-frame sync</span>
        </span>
        <span className="value">
          <i className="dot" /> <b>Conflict-free</b> <span>operational transforms</span>
        </span>
        <span className="value">
          <i className="dot" /> <b>No accounts</b> <span>the link is the key</span>
        </span>
      </div>
      {error ? <p className="error">{error}</p> : null}
      <footer className="footer">
        Rust · WebSockets · Operational Transforms ·{" "}
        <a href="https://github.com/ayenisholah/SyncPad">GitHub</a>
      </footer>
    </main>
  );
}
