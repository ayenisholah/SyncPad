import { useCallback, useEffect, useRef, useState } from "react";
import MonacoEditor, { type OnMount } from "@monaco-editor/react";
import type * as monaco from "monaco-editor";
import { TextOperation, diffToOperation, operationToEdits, utf16ToCodePoint } from "./ops";
import {
  Connection,
  docSocketUrl,
  type ConnectionStatus,
  type Participant,
} from "./connection";
import { RemoteCursors } from "./cursors";

/** Throttle interval for outgoing cursor updates (spec §6.2). */
const CURSOR_THROTTLE_MS = 50;

const STATUS_LABEL: Record<ConnectionStatus, string> = {
  connecting: "connecting…",
  open: "connected",
  closed: "reconnecting…",
};

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
    const connection = new Connection(docSocketUrl(docId), {
      onInit: (state) => {
        seedContent(state.content);
        setLanguage(state.language);
        selfIdRef.current = state.selfId;
        participantsRef.current = new Map(state.participants.map((p) => [p.id, p]));
        remoteCursorsRef.current?.clear();
      },
      onApplyOperation: applyRemote,
      onPresence: (joined, left) => {
        if (joined) participantsRef.current.set(joined.id, joined);
        if (left) {
          participantsRef.current.delete(left);
          remoteCursorsRef.current?.remove(left);
        }
      },
      onCursor: (authorId, position, selection) => {
        if (authorId === selfIdRef.current) return;
        const peer = participantsRef.current.get(authorId);
        if (!peer) return;
        remoteCursorsRef.current?.set(authorId, peer.name, peer.color, position, selection);
      },
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

  return (
    <div className="editor-shell">
      <header className="editor-topbar">
        <span className="wordmark-sm">SyncPad</span>
        <code className="doc-slug">d/{docId}</code>
        <button className="copy-link" onClick={copyLink}>
          {copied ? "copied" : "copy link"}
        </button>
        <span className="spacer" />
        <span className="lang-tag">{language}</span>
      </header>
      <div className="editor-main">
        <MonacoEditor
          theme="vs-dark"
          language={language}
          defaultValue=""
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
        {STATUS_LABEL[status]}
      </footer>
    </div>
  );
}
