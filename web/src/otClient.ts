// Client-side OT state machine (the ot.js Client pattern, spec §6.1).
//
// One operation is in flight at a time. Local edits are already present in the
// editor, so they are only *sent* (or composed into a buffer while an
// operation is awaiting confirmation) — never re-applied. Incoming server
// operations are transformed through the pending pipeline before being applied
// to the editor. This mirrors the `SimClient` in
// `server/tests/fuzz_convergence.rs`, the convergence reference.

import { TextOperation } from "./ops";

export interface OtClientCallbacks {
  /** Send `operation` to the server, based on `revision`. */
  sendOperation(revision: number, operation: TextOperation): void;
  /** Apply a transformed remote `operation` to the editor. */
  applyOperation(operation: TextOperation): void;
}

export type StateName = "synchronized" | "awaitingConfirm" | "awaitingWithBuffer";

interface ClientState {
  readonly name: StateName;
  applyClient(client: OtClient, operation: TextOperation): ClientState;
  applyServer(client: OtClient, operation: TextOperation): ClientState;
  serverAck(client: OtClient): ClientState;
}

/** No pending operations: the client is in sync with the server. */
class Synchronized implements ClientState {
  readonly name = "synchronized" as const;

  applyClient(client: OtClient, operation: TextOperation): ClientState {
    client.sendOperation(client.revision, operation);
    return new AwaitingConfirm(operation);
  }

  applyServer(client: OtClient, operation: TextOperation): ClientState {
    client.applyOperation(operation);
    return this;
  }

  serverAck(): ClientState {
    throw new Error("serverAck called with no operation in flight");
  }
}

/** One operation sent, awaiting its ack; no local edits since. */
class AwaitingConfirm implements ClientState {
  readonly name = "awaitingConfirm" as const;

  constructor(private readonly outstanding: TextOperation) {}

  applyClient(_client: OtClient, operation: TextOperation): ClientState {
    // Hold the local edit in a buffer until the outstanding op is confirmed.
    return new AwaitingWithBuffer(this.outstanding, operation);
  }

  applyServer(client: OtClient, operation: TextOperation): ClientState {
    const [outstandingPrime, operationPrime] = TextOperation.transform(this.outstanding, operation);
    client.applyOperation(operationPrime);
    return new AwaitingConfirm(outstandingPrime);
  }

  serverAck(): ClientState {
    return new Synchronized();
  }
}

/** One operation in flight plus buffered local edits composed together. */
class AwaitingWithBuffer implements ClientState {
  readonly name = "awaitingWithBuffer" as const;

  constructor(
    private readonly outstanding: TextOperation,
    private readonly buffer: TextOperation,
  ) {}

  applyClient(_client: OtClient, operation: TextOperation): ClientState {
    return new AwaitingWithBuffer(this.outstanding, this.buffer.compose(operation));
  }

  applyServer(client: OtClient, operation: TextOperation): ClientState {
    const [outstandingPrime, operation1] = TextOperation.transform(this.outstanding, operation);
    const [bufferPrime, operation2] = TextOperation.transform(this.buffer, operation1);
    client.applyOperation(operation2);
    return new AwaitingWithBuffer(outstandingPrime, bufferPrime);
  }

  serverAck(client: OtClient): ClientState {
    // The outstanding op is confirmed; the buffer becomes the next in flight.
    client.sendOperation(client.revision, this.buffer);
    return new AwaitingConfirm(this.buffer);
  }
}

export class OtClient {
  private state: ClientState = new Synchronized();

  constructor(
    public revision: number,
    private readonly callbacks: OtClientCallbacks,
  ) {}

  /** The name of the current state (for tests and status display). */
  get stateName(): StateName {
    return this.state.name;
  }

  /** A local edit was made in the editor. */
  applyClient(operation: TextOperation): void {
    this.state = this.state.applyClient(this, operation);
  }

  /** A remote operation arrived from the server at the next revision. */
  applyServer(operation: TextOperation): void {
    this.revision += 1;
    this.state = this.state.applyServer(this, operation);
  }

  /** The server acknowledged the outstanding operation. */
  serverAck(): void {
    this.revision += 1;
    this.state = this.state.serverAck(this);
  }

  /** Internal: forwarded to the send callback by the states. */
  sendOperation(revision: number, operation: TextOperation): void {
    this.callbacks.sendOperation(revision, operation);
  }

  /** Internal: forwarded to the apply callback by the states. */
  applyOperation(operation: TextOperation): void {
    this.callbacks.applyOperation(operation);
  }
}
