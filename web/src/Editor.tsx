import { useCallback, useEffect, useRef, useState } from "react";
import MonacoEditor, { type BeforeMount, type OnMount } from "@monaco-editor/react";
import type * as monaco from "monaco-editor";
import { TextOperation, diffToOperation, operationToEdits, utf16ToCodePoint } from "./ops";
import {
  Connection,
  docSocketUrl,
  type ConnectionStatus,
  type Participant,
} from "./connection";
import { RemoteCursors } from "./cursors";
import { ShareModal } from "./ShareModal";

/** Throttle interval for outgoing cursor updates (spec §6.2). */
const CURSOR_THROTTLE_MS = 50;
/** How often the status bar refreshes revision + latency. */
const STATUS_POLL_MS = 500;

const STATUS_LABEL: Record<ConnectionStatus, string> = {
  connecting: "connecting…",
  open: "connected",
  closed: "reconnecting…",
};

/** Language picker options; ids must match the server allowlist (spec FR7). */
const LANGUAGES: { id: string; label: string }[] = [
  { id: "plaintext", label: "Plain text" },
  { id: "javascript", label: "JavaScript" },
  { id: "typescript", label: "TypeScript" },
  { id: "python", label: "Python" },
  { id: "rust", label: "Rust" },
  { id: "go", label: "Go" },
  { id: "java", label: "Java" },
  { id: "c", label: "C" },
  { id: "cpp", label: "C++" },
  { id: "csharp", label: "C#" },
  { id: "json", label: "JSON" },
  { id: "yaml", label: "YAML" },
  { id: "markdown", label: "Markdown" },
  { id: "html", label: "HTML" },
  { id: "css", label: "CSS" },
  { id: "sql", label: "SQL" },
  { id: "shell", label: "Shell" },
  { id: "ruby", label: "Ruby" },
  { id: "php", label: "PHP" },
];

/** Monaco range covering a UTF-16 [offset, offset+length) span. */
function spanToRange(
  model: monaco.editor.ITextModel,
  offset: number,
  length: number,
): monaco.IRange {
  const start = model.getPositionAt(offset);
  const end = model.getPositionAt(offset + length);
  return {
    startLineNumber: start.lineNumber,
    startColumn: start.column,
    endLineNumber: end.lineNumber,
    endColumn: end.column,
  };
}

export function Editor({ docId }: { docId: string }) {
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const connectionRef = useRef<Connection | null>(null);
  // Guards Monaco's change listener while we apply remote edits, so a remote
  // op is never echoed back to the server (the classic OT loop, spec §6.3).
  const applyingRemote = useRef(false);
  // True while an IME is composing; outgoing ops are held until the composition
  // ends so half-composed input is never sent or transformed (spec §6.3).
  const composing = useRef(false);
  // The last content we and the server agree on, for diffing local edits and
  // mapping remote-op offsets. Kept in sync with the model's LF-normalized text.
  const contentRef = useRef("");
  // Remote cursor decorations and the roster that names/colors them.
  const remoteCursorsRef = useRef<RemoteCursors | null>(null);
  const participantsRef = useRef<Map<string, Participant>>(new Map());
  const selfIdRef = useRef("");

  const [status, setStatus] = useState<ConnectionStatus>("connecting");
  const [language, setLanguage] = useState("plaintext");
  const [copied, setCopied] = useState(false);
  const [participants, setParticipants] = useState<Participant[]>([]);
  const [revision, setRevision] = useState(0);
  const [latency, setLatency] = useState<number | null>(null);
  // The snippet captured when the Share panel is opened (selection or whole doc).
  const [share, setShare] = useState<{ selection: string; whole: string } | null>(null);

  const seedContent = useCallback((content: string) => {
    const model = editorRef.current?.getModel();
    contentRef.current = content;
    if (!model) return; // onMount will seed from contentRef
    applyingRemote.current = true;
    try {
      if (model.getValue() !== content) model.setValue(content);
    } finally {
      applyingRemote.current = false;
    }
  }, []);

  const applyRemote = useCallback((operation: TextOperation) => {
    const editor = editorRef.current;
    const model = editor?.getModel();
    const doc = contentRef.current;
    if (!editor || !model) {
      // No editor yet: fold the op into the pending content.
      contentRef.current = operation.apply(doc);
      return;
    }
    const edits = operationToEdits(operation, doc).map((span) => ({
      range: spanToRange(model, span.offset, span.length),
      text: span.text,
      forceMoveMarkers: true,
    }));
    applyingRemote.current = true;
    try {
      editor.executeEdits("remote", edits);
    } finally {
      applyingRemote.current = false;
    }
    contentRef.current = operation.apply(doc);
  }, []);

  useEffect(() => {
    const syncRoster = () => setParticipants([...participantsRef.current.values()]);
    const connection = new Connection(docSocketUrl(docId), {
      onInit: (state) => {
        seedContent(state.content);
        setLanguage(state.language);
        selfIdRef.current = state.selfId;
        participantsRef.current = new Map(state.participants.map((p) => [p.id, p]));
        remoteCursorsRef.current?.clear();
        syncRoster();
      },
      onApplyOperation: applyRemote,
      onPresence: (joined, left) => {
        if (joined) participantsRef.current.set(joined.id, joined);
        if (left) {
          participantsRef.current.delete(left);
          remoteCursorsRef.current?.remove(left);
        }
        syncRoster();
      },
      onCursor: (authorId, position, selection) => {
        if (authorId === selfIdRef.current) return;
        const peer = participantsRef.current.get(authorId);
        if (!peer) return;
        remoteCursorsRef.current?.set(authorId, peer.name, peer.color, position, selection);
      },
      onLanguage: setLanguage,
      onStatus: setStatus,
    });
    connectionRef.current = connection;
    connection.connect();
    return () => {
      connection.close();
      remoteCursorsRef.current?.dispose();
      remoteCursorsRef.current = null;
    };
  }, [docId, applyRemote, seedContent]);

  // Poll the live status readout (revision + rolling op→apply latency, spec §8.1).
  useEffect(() => {
    const id = setInterval(() => {
      const connection = connectionRef.current;
      if (!connection) return;
      setRevision(connection.revision);
      setLatency(connection.latencyP50());
    }, STATUS_POLL_MS);
    return () => clearInterval(id);
  }, []);

  const changeLanguage = (next: string) => {
    setLanguage(next);
    connectionRef.current?.sendLanguage(next);
  };

  // Match the editor surface to the app background (defined before mount so
  // there's no flash of the stock vs-dark grey).
  const handleBeforeMount: BeforeMount = (monacoApi) => {
    monacoApi.editor.defineTheme("syncpad", {
      base: "vs-dark",
      inherit: true,
      rules: [],
      colors: {
        "editor.background": "#0d0b14",
        "editorGutter.background": "#0d0b14",
        "editor.lineHighlightBackground": "#17131f",
        "editorLineNumber.foreground": "#4c4568",
        "editorLineNumber.activeForeground": "#a78bfa",
        "editor.selectionBackground": "#3a2f66",
        "editorCursor.foreground": "#a78bfa",
        "editorWidget.background": "#17131f",
        "editorWidget.border": "#2a2440",
      },
    });
  };

  const handleMount: OnMount = (editor, monacoApi) => {
    editorRef.current = editor;
    const model = editor.getModel();
    if (!model) return;
    // Force `\n` so client and server agree on character offsets (spec §6.3).
    model.setEOL(monacoApi.editor.EndOfLineSequence.LF);
    // Seed from any init that arrived before the editor mounted.
    if (contentRef.current && model.getValue() !== contentRef.current) {
      applyingRemote.current = true;
      model.setValue(contentRef.current);
      applyingRemote.current = false;
    }
    const flushLocalChange = () => {
      const next = model.getValue();
      const op = diffToOperation(contentRef.current, next);
      if (op.isNoop()) return;
      contentRef.current = next;
      connectionRef.current?.submit(op);
    };

    model.onDidChangeContent(() => {
      // Remote edits are applied under the echo guard; IME intermediate states
      // are held until composition ends.
      if (applyingRemote.current || composing.current) return;
      flushLocalChange();
    });

    editor.onDidCompositionStart(() => {
      composing.current = true;
    });
    editor.onDidCompositionEnd(() => {
      composing.current = false;
      // Collapse the whole composition into one operation.
      if (!applyingRemote.current) flushLocalChange();
    });

    // Remote-cursor decorations, and our own caret reported to peers (FR5).
    remoteCursorsRef.current = new RemoteCursors(editor, monacoApi);

    let cursorTimer: ReturnType<typeof setTimeout> | null = null;
    const reportCursor = () => {
      const selection = editor.getSelection();
      if (!selection) return;
      const text = model.getValue();
      const head = utf16ToCodePoint(text, model.getOffsetAt(selection.getPosition()));
      const anchor = utf16ToCodePoint(text, model.getOffsetAt(selection.getSelectionStart()));
      connectionRef.current?.sendCursor(head, anchor !== head ? { anchor, head } : undefined);
    };
    editor.onDidChangeCursorSelection(() => {
      // Trailing throttle: coalesce rapid movements into one send per interval.
      if (cursorTimer) return;
      cursorTimer = setTimeout(() => {
        cursorTimer = null;
        reportCursor();
      }, CURSOR_THROTTLE_MS);
    });
  };

  const copyLink = () => {
    void navigator.clipboard?.writeText(window.location.href).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  const openShare = () => {
    const editor = editorRef.current;
    const model = editor?.getModel();
    if (!model) return;
    const selection = editor?.getSelection();
    const selected =
      selection && !selection.isEmpty() ? model.getValueInRange(selection) : "";
    setShare({ selection: selected, whole: model.getValue() });
  };

  return (
    <div className="editor-shell">
      <header className="editor-topbar">
        <span className="brand">
          <img className="brand-mark" src="/favicon.svg" alt="" width={22} height={22} />
          <span className="wordmark-sm">SyncPad</span>
        </span>
        <code className="doc-slug">d/{docId}</code>
        <button className="copy-link" onClick={copyLink}>
          {copied ? "copied" : "copy link"}
        </button>
        <button className="share-btn" onClick={openShare}>
          Share
        </button>
        <span className="spacer" />
        <div className="topbar-right">
          <div className="presence" title="People in this document">
            <span className="avatar avatar-you" title="You">
              you
            </span>
            {participants.map((p) => (
              <span
                key={p.id}
                className="avatar"
                style={{ background: p.color }}
                title={p.name}
              >
                {p.name.slice(0, 1).toUpperCase()}
              </span>
            ))}
          </div>
          <span className="lang-picker">
            <select
              className="lang-select"
              value={language}
              onChange={(e) => changeLanguage(e.target.value)}
              aria-label="Language"
            >
              {LANGUAGES.map((l) => (
                <option key={l.id} value={l.id}>
                  {l.label}
                </option>
              ))}
            </select>
          </span>
        </div>
      </header>
      <div className="editor-main">
        <MonacoEditor
          theme="syncpad"
          language={language}
          defaultValue=""
          beforeMount={handleBeforeMount}
          onMount={handleMount}
          options={{
            fontFamily: '"JetBrains Mono", Consolas, monospace',
            fontSize: 14,
            minimap: { enabled: false },
            automaticLayout: true,
            scrollBeyondLastLine: false,
          }}
        />
      </div>
      <footer className="editor-status">
        <span className={`status-dot status-${status}`} />
        <span>{STATUS_LABEL[status]}</span>
        <span className="status-sep">·</span>
        <span className="status-latency">
          sync {latency === null ? "—" : `${latency} ms`}
        </span>
        <span className="status-sep">·</span>
        <span className="status-rev">rev {revision}</span>
      </footer>
      {share && (
        <ShareModal
          docUrl={window.location.href}
          language={language}
          languageLabel={LANGUAGES.find((l) => l.id === language)?.label ?? language}
          selection={share.selection}
          whole={share.whole}
          onClose={() => setShare(null)}
        />
      )}
    </div>
  );
}
