import { describe, expect, it } from "vitest";
import { parseArgs, percentile, summarize } from "./stressMetrics";

describe("stress harness", () => {
  it("parses explicit ramp options", () => {
    expect(parseArgs(["--origin", "https://syncpad.example", "--start-sessions", "2", "--step-sessions", "3", "--max-sessions", "8", "--hold-seconds", "10", "--operation-interval-ms", "250"])).toMatchObject({
      origin: "https://syncpad.example", startSessions: 2, stepSessions: 3, maxSessions: 8, holdSeconds: 10, operationIntervalMs: 250,
    });
  });

  it("restricts synthetic addresses to loopback", () => {
    expect(() => parseArgs(["--origin", "https://syncpad.example", "--synthetic-ips"])).toThrow(/loopback/);
  });

  it("calculates nearest-rank percentiles", () => {
    expect(percentile([9, 1, 5, 3], 0.5)).toBe(3);
    expect(percentile([9, 1, 5, 3], 0.95)).toBe(9);
    expect(percentile([], 0.5)).toBeNull();
  });

  it("marks only healthy steps stable", () => {
    const base = { sessions: 2, sent: 100, acknowledged: 99, received: 99, errors: 0, disconnects: 0, convergenceFailures: 0, latencies: [10, 20, 30] };
    expect(summarize(base, 10).stable).toBe(true);
    expect(summarize({ ...base, convergenceFailures: 1 }, 10).stable).toBe(false);
    expect(summarize({ ...base, latencies: [300] }, 10).stable).toBe(false);
  });
});
