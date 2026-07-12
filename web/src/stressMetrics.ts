export interface StressOptions {
  origin: string;
  startSessions: number;
  stepSessions: number;
  maxSessions: number;
  holdSeconds: number;
  operationIntervalMs: number;
  syntheticIps: boolean;
}

export interface StepMetrics {
  sessions: number;
  sent: number;
  acknowledged: number;
  received: number;
  errors: number;
  disconnects: number;
  convergenceFailures: number;
  latencies: number[];
}

export interface StepSummary {
  sessions: number;
  sent: number;
  acknowledged: number;
  received: number;
  errors: number;
  disconnects: number;
  convergenceFailures: number;
  acknowledgementRate: number;
  operationsPerSecond: number;
  p50Ms: number | null;
  p95Ms: number | null;
  stable: boolean;
}

const DEFAULTS: StressOptions = { origin: "http://127.0.0.1:8090", startSessions: 10, stepSessions: 10, maxSessions: 100, holdSeconds: 30, operationIntervalMs: 500, syntheticIps: false };

function positiveInteger(value: string | undefined, flag: string): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) throw new Error(`${flag} must be a positive integer`);
  return parsed;
}

export function parseArgs(args: string[]): StressOptions {
  const options = { ...DEFAULTS };
  for (let i = 0; i < args.length; i += 1) {
    const flag = args[i];
    if (flag === "--synthetic-ips") options.syntheticIps = true;
    else if (flag === "--origin") options.origin = args[++i] ?? "";
    else if (flag === "--start-sessions") options.startSessions = positiveInteger(args[++i], flag);
    else if (flag === "--step-sessions") options.stepSessions = positiveInteger(args[++i], flag);
    else if (flag === "--max-sessions") options.maxSessions = positiveInteger(args[++i], flag);
    else if (flag === "--hold-seconds") options.holdSeconds = positiveInteger(args[++i], flag);
    else if (flag === "--operation-interval-ms") options.operationIntervalMs = positiveInteger(args[++i], flag);
    else throw new Error(`unknown argument: ${flag}`);
  }
  const origin = new URL(options.origin);
  if (origin.protocol !== "http:" && origin.protocol !== "https:") throw new Error("--origin must use http or https");
  if (options.startSessions > options.maxSessions) throw new Error("--start-sessions cannot exceed --max-sessions");
  if (options.syntheticIps && origin.hostname !== "127.0.0.1" && origin.hostname !== "localhost") throw new Error("--synthetic-ips is restricted to a loopback origin");
  return options;
}

export function percentile(samples: number[], quantile: number): number | null {
  if (samples.length === 0) return null;
  const sorted = [...samples].sort((a, b) => a - b);
  return sorted[Math.ceil(quantile * sorted.length) - 1];
}

export function summarize(metrics: StepMetrics, holdSeconds: number): StepSummary {
  const acknowledgementRate = metrics.sent === 0 ? 0 : metrics.acknowledged / metrics.sent;
  const p50Ms = percentile(metrics.latencies, 0.5);
  const p95Ms = percentile(metrics.latencies, 0.95);
  const { latencies: _latencies, ...counts } = metrics;
  return { ...counts, acknowledgementRate, operationsPerSecond: metrics.acknowledged / holdSeconds, p50Ms, p95Ms, stable: metrics.sent > 0 && acknowledgementRate >= 0.99 && metrics.disconnects === 0 && metrics.convergenceFailures === 0 && p95Ms !== null && p95Ms < 250 };
}
