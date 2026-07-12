import process from "node:process";
// The harness is launched through the web workspace, which owns its tooling.
// A relative import keeps the repository single-package while allowing esbuild
// to produce a dependency-free artifact for the VPS measurement job.
import WebSocket from "../web/node_modules/ws/wrapper.mjs";
import { parseArgs, summarize, type StepMetrics, type StressOptions } from "../web/src/stressMetrics";

type ServerMessage = { type: string; revision?: number; content?: string; ops?: Array<number | string>; sentAt?: number };
type MetricsRef = { current: StepMetrics };

class Peer {
  socket!: WebSocket;
  revision = 0;
  content = "";
  ready!: Promise<void>;
  private resolveReady!: () => void;
  private pendingAck: (() => void) | null = null;

  constructor(
    private readonly url: string,
    private readonly headers: Record<string, string>,
    private readonly metrics: MetricsRef,
    private readonly onRemote: (sentAt: number) => void,
  ) {
    this.ready = new Promise((resolve) => { this.resolveReady = resolve; });
  }

  connect(): void {
    this.socket = new WebSocket(this.url, { headers: this.headers });
    this.socket.on("message", (data) => this.receive(data.toString()));
    this.socket.on("error", () => { this.metrics.current.errors += 1; });
    this.socket.on("close", () => { this.metrics.current.disconnects += 1; });
  }

  private receive(raw: string): void {
    let message: ServerMessage;
    try { message = JSON.parse(raw) as ServerMessage; }
    catch { this.metrics.current.errors += 1; return; }
    if (message.type === "init") {
      this.revision = message.revision ?? 0;
      this.content = message.content ?? "";
      this.resolveReady();
    } else if (message.type === "ack") {
      this.revision = message.revision ?? this.revision;
      this.metrics.current.acknowledged += 1;
      this.pendingAck?.();
      this.pendingAck = null;
    } else if (message.type === "op") {
      const ops = message.ops ?? [];
      const inserted = ops.find((part): part is string => typeof part === "string") ?? "";
      this.content += inserted;
      this.revision = message.revision ?? this.revision;
      this.metrics.current.received += 1;
      if (typeof message.sentAt === "number") this.onRemote(message.sentAt);
    } else if (message.type === "resync") {
      this.metrics.current.errors += 1;
    }
  }

  append(token: string): Promise<void> {
    if (this.pendingAck) throw new Error("peer already has an operation in flight");
    const sentAt = Date.now();
    const ops: Array<number | string> = this.content.length === 0 ? [token] : [this.content.length, token];
    this.content += token;
    this.metrics.current.sent += 1;
    this.socket.send(JSON.stringify({ type: "op", baseRevision: this.revision, ops, sentAt }));
    return new Promise((resolve) => { this.pendingAck = resolve; });
  }

  close(): void { this.socket.close(); }
}

class Session {
  private turn = 0;
  private sequence = 0;
  private stopped = false;
  private paused = false;
  private readonly metricsRef: MetricsRef;
  readonly peers: [Peer, Peer];

  constructor(url: string, headers: Record<string, string>, metrics: StepMetrics) {
    this.metricsRef = { current: metrics };
    const remote = (sentAt: number) => this.metricsRef.current.latencies.push(Date.now() - sentAt);
    this.peers = [new Peer(url, headers, this.metricsRef, remote), new Peer(url, headers, this.metricsRef, remote)];
  }

  async start(intervalMs: number): Promise<void> {
    this.peers.forEach((peer) => peer.connect());
    await Promise.all(this.peers.map((peer) => peer.ready));
    void this.loop(intervalMs);
  }

  private async loop(intervalMs: number): Promise<void> {
    while (!this.stopped) {
      if (this.paused) { await new Promise((resolve) => setTimeout(resolve, 10)); continue; }
      const peer = this.peers[this.turn];
      try {
        await peer.append(`x${this.sequence.toString(36)} `);
        this.sequence += 1;
        this.turn = 1 - this.turn;
      } catch { this.metricsRef.current.errors += 1; }
      await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
  }

  verify(): void {
    if (this.peers[0].content !== this.peers[1].content || this.peers[0].revision !== this.peers[1].revision) {
      this.metricsRef.current.convergenceFailures += 1;
    }
  }

  setMetrics(metrics: StepMetrics): void { this.metricsRef.current = metrics; }
  setPaused(paused: boolean): void { this.paused = paused; }
  stop(): void { this.stopped = true; this.peers.forEach((peer) => peer.close()); }
}

async function createDocument(origin: string): Promise<string> {
  const response = await fetch(new URL("/api/docs", origin), { method: "POST" });
  if (!response.ok) throw new Error(`create document failed: HTTP ${response.status}`);
  const body = await response.json() as { docId?: string };
  if (!body.docId) throw new Error("create document response omitted docId");
  return body.docId;
}

function websocketUrl(origin: string, docId: string): string {
  const url = new URL(`/ws/${docId}`, origin);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.toString();
}

async function run(options: StressOptions): Promise<void> {
  const sessions: Session[] = [];
  let previousTarget = 0;
  for (let target = options.startSessions; target <= options.maxSessions; target += options.stepSessions) {
    sessions.forEach((session) => session.setPaused(true));
    let metrics: StepMetrics = { sessions: target, sent: 0, acknowledged: 0, received: 0, errors: 0, disconnects: 0, convergenceFailures: 0, latencies: [] };
    sessions.forEach((session) => session.setMetrics(metrics));
    for (let i = previousTarget; i < target; i += 1) {
      const docId = await createDocument(options.origin);
      const headers = options.syntheticIps ? { "X-Real-IP": `198.18.${Math.floor(i / 250)}.${(i % 250) + 1}` } : {};
      const session = new Session(websocketUrl(options.origin, docId), headers, metrics);
      await session.start(options.operationIntervalMs);
      session.setPaused(true);
      sessions.push(session);
    }
    previousTarget = target;
    await new Promise((resolve) => setTimeout(resolve, options.operationIntervalMs + 100));
    // Exclude ramp setup from the hold-period statistics.
    metrics = { sessions: target, sent: 0, acknowledged: 0, received: 0, errors: 0, disconnects: 0, convergenceFailures: 0, latencies: [] };
    sessions.forEach((session) => { session.setMetrics(metrics); session.setPaused(false); });
    await new Promise((resolve) => setTimeout(resolve, options.holdSeconds * 1000));
    sessions.forEach((session) => session.setPaused(true));
    await new Promise((resolve) => setTimeout(resolve, options.operationIntervalMs + 100));
    sessions.forEach((session) => session.verify());
    const summary = summarize(metrics, options.holdSeconds);
    process.stdout.write(`${JSON.stringify(summary)}\n`);
    if (metrics.errors > 0 || metrics.disconnects > 0 || metrics.convergenceFailures > 0 || summary.acknowledgementRate < 0.99) {
      process.exitCode = 1;
    }
    if (!summary.stable) break;
    sessions.forEach((session) => session.setPaused(false));
  }
  sessions.forEach((session) => session.stop());
}

run(parseArgs(process.argv.slice(2))).catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
