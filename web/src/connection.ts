// WebSocket client: bridges the editor and the server's sync protocol
// (spec §6.2), driving the OT state machine. Local edits are submitted here and
// sent to the server; incoming server messages advance the state machine and
// surface remote operations for the editor to apply.
//
// The socket is injectable so the message handling can be unit-tested without a
// network; production uses the global `WebSocket`.

import { OtClient } from "./otClient";
import { TextOperation, type SerializedOperation } from "./ops";

export interface Participant {
  id: string;
  name: string;
  color: string;
}

/** The initial document state delivered on connect and on forced resync. */
export interface InitState {
  revision: number;
  content: string;
  language: string;
  participants: Participant[];
  selfId: string;
}

export type ConnectionStatus = "connecting" | "open" | "closed";

/** A selection range in character offsets (anchor may follow head). */
export interface Selection {
  anchor: number;
  head: number;
}

export interface ConnectionHandlers {
  /** Fresh document state: the editor should reset to `content`. */
  onInit(state: InitState): void;
  /** A transformed remote operation to apply to the editor. */
  onApplyOperation(operation: TextOperation): void;
  /** Roster delta (spec FR6). */
  onPresence?(joined: Participant | undefined, left: string | undefined): void;
  /** A peer's caret/selection at the current revision (spec FR5). */
  onCursor?(authorId: string, position: number, selection: Selection | undefined): void;
  /** The document language changed (spec FR7). */
  onLanguage?(language: string): void;
  /** The server forced a resync; editor state will be replaced by the next init. */
  onResync?(): void;
  /** Connection lifecycle, for the status bar. */
  onStatus?(status: ConnectionStatus): void;
}

/** The subset of `WebSocket` this client uses (so a fake can stand in). */
export interface SocketLike {
  send(data: string): void;
  close(): void;
  onopen: ((event: unknown) => void) | null;
  onclose: ((event: unknown) => void) | null;
  onerror: ((event: unknown) => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
}

export type SocketFactory = (url: string) => SocketLike;

interface ServerMessage {
  type: string;
  [key: string]: unknown;
}

const RECONNECT_DELAY_MS = 1000;
const LATENCY_SAMPLES = 50;

export class Connection {
  private socket: SocketLike | null = null;
  private client: OtClient | null = null;
  private closedByCaller = false;
  // Rolling op→apply latencies (ms), measured on received remote ops (spec §6.5).
  private latencies: number[] = [];

  constructor(
    private readonly url: string,
    private readonly handlers: ConnectionHandlers,
    private readonly socketFactory: SocketFactory = (url) =>
      new WebSocket(url) as unknown as SocketLike,
  ) {}

  /** Open the socket (and reconnect on drop until `close` is called). */
  connect(): void {
    this.closedByCaller = false;
    this.open();
  }

  private open(): void {
    this.handlers.onStatus?.("connecting");
    const socket = this.socketFactory(this.url);
    this.socket = socket;
    socket.onopen = () => this.handlers.onStatus?.("open");
    socket.onmessage = (event) => {
      if (typeof event.data === "string") this.receive(event.data);
    };
    socket.onclose = () => {
      this.handlers.onStatus?.("closed");
      this.client = null;
      if (!this.closedByCaller) {
        setTimeout(() => {
          if (!this.closedByCaller) this.open();
        }, RECONNECT_DELAY_MS);
      }
    };
    socket.onerror = () => socket.close();
  }

  /** Close the connection and stop reconnecting. */
  close(): void {
    this.closedByCaller = true;
    this.socket?.close();
    this.socket = null;
    this.client = null;
  }

  /**
   * Submit a local edit. Safe to call only after `onInit`; before the state
   * machine exists the edit is dropped (the editor is seeded from init and
   * will not have produced edits yet).
   */
  submit(operation: TextOperation): void {
    this.client?.applyClient(operation);
  }

  /** Report the local caret/selection to the server (spec FR5). */
  sendCursor(position: number, selection?: Selection): void {
    this.socket?.send(
      JSON.stringify({ type: "cursor", position, selection: selection ?? null }),
    );
  }

  /** Request a document language change (spec FR7). */
  sendLanguage(language: string): void {
    this.socket?.send(JSON.stringify({ type: "setLanguage", language }));
  }

  /** The revision the client currently believes it is at. */
  get revision(): number {
    return this.client?.revision ?? 0;
  }

  /** Median op→apply latency over the rolling window, or null if no samples. */
  latencyP50(): number | null {
    if (this.latencies.length === 0) return null;
    const sorted = [...this.latencies].sort((a, b) => a - b);
    return sorted[Math.floor(sorted.length / 2)];
  }

  private receive(raw: string): void {
    let message: ServerMessage;
    try {
      message = JSON.parse(raw) as ServerMessage;
    } catch {
      return; // ignore malformed frames
    }

    switch (message.type) {
      case "init": {
        const init = message as unknown as InitState & { type: string };
        this.client = new OtClient(init.revision, {
          sendOperation: (revision, op) => this.sendOperation(revision, op),
          applyOperation: (op) => this.handlers.onApplyOperation(op),
        });
        this.handlers.onInit({
          revision: init.revision,
          content: init.content,
          language: init.language,
          participants: init.participants,
          selfId: init.selfId,
        });
        break;
      }
      case "op": {
        if (!this.client) break;
        // Sample op→apply latency: sender's clock to now (spec §6.5).
        const sentAt = message.sentAt as number | undefined;
        if (typeof sentAt === "number") {
          this.latencies.push(Math.max(0, Date.now() - sentAt));
          if (this.latencies.length > LATENCY_SAMPLES) this.latencies.shift();
        }
        const op = TextOperation.fromJSON(message.ops as SerializedOperation);
        this.client.applyServer(op);
        break;
      }
      case "ack": {
        this.client?.serverAck();
        break;
      }
      case "resync": {
        // Drop local state; the server follows this with a fresh init.
        this.client = null;
        this.handlers.onResync?.();
        break;
      }
      case "presence": {
        this.handlers.onPresence?.(
          message.joined as Participant | undefined,
          message.left as string | undefined,
        );
        break;
      }
      case "cursor": {
        this.handlers.onCursor?.(
          message.authorId as string,
          message.position as number,
          (message.selection as Selection | null) ?? undefined,
        );
        break;
      }
      case "language": {
        this.handlers.onLanguage?.(message.language as string);
        break;
      }
      // pong arrives with the skew-correction feature (W2D4-2).
      default:
        break;
    }
  }

  private sendOperation(revision: number, operation: TextOperation): void {
    this.socket?.send(
      JSON.stringify({
        type: "op",
        baseRevision: revision,
        ops: operation.toJSON(),
        sentAt: Date.now(),
      }),
    );
  }
}

/** Build the WebSocket URL for a document id from the current page origin. */
export function docSocketUrl(docId: string): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/ws/${docId}`;
}
