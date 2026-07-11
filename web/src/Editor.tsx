import { useCallback, useEffect, useRef, useState } from "react";
import MonacoEditor, { type OnMount } from "@monaco-editor/react";
import type * as monaco from "monaco-editor";
import { TextOperation, diffToOperation, operationToEdits } from "./ops";
import { Connection, docSocketUrl, type ConnectionStatus } from "./connection";

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
  // The last content we and the server agree on, for diffing local edits and
  // mapping remote-op offsets. Kept in sync with the model's LF-normalized text.
  const contentRef = useRef("");

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
      },
      onApplyOperation: applyRemote,
      onStatus: setStatus,
    });
    connectionRef.current = connection;
    connection.connect();
    return () => connection.close();
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
    model.onDidChangeContent(() => {
      if (applyingRemote.current) return;
      const next = model.getValue();
      const op = diffToOperation(contentRef.current, next);
      if (op.isNoop()) return;
      contentRef.current = next;
      connectionRef.current?.submit(op);
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
