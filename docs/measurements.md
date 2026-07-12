# Production Measurements

Measured values are reported separately for the public network path and the
server's loopback path. Loopback results describe server capacity and processing
latency; they are not internet latency claims.

## Server capacity

Measured on 2026-07-12 with the `Measure Production` workflow introduced in
commit `fe62b76`. The harness ran on the production VPS against
`http://127.0.0.1:8090`, bypassing nginx and TLS but exercising the deployed
SyncPad server. Synthetic source addresses were used only on loopback so the
production limit of 10 active documents per public IP remained enabled.

Each collaborative session had two WebSocket clients sharing one document.
Every session submitted one operation every 500 ms. Load increased by 20
sessions per step and each step ran for 30 seconds. A stable step required at
least 99% acknowledgements, no unexpected disconnects, no convergence failures,
and p95 remote-apply latency below 250 ms.

| Sessions | Clients | Acked ops/s | Ack rate | p50 | p95 | Failures | Stable |
|---:|---:|---:|---:|---:|---:|---:|:---:|
| 20 | 40 | 40 | 100% | 1 ms | 3 ms | 0 | Yes |
| 40 | 80 | 80 | 100% | 1 ms | 3 ms | 0 | Yes |
| 60 | 120 | 120 | 100% | 1 ms | 3 ms | 0 | Yes |
| 80 | 160 | 160 | 100% | 1 ms | 2 ms | 0 | Yes |
| 100 | 200 | 200 | 100% | 1 ms | 3 ms | 0 | Yes |
| 120 | 240 | 240 | 100% | 1 ms | 3 ms | 0 | Yes |
| 140 | 280 | 280 | 100% | 1 ms | 3 ms | 0 | Yes |
| 160 | 320 | 320 | 100% | 1 ms | 3 ms | 0 | Yes |
| 180 | 360 | 360 | 100% | 1 ms | 3 ms | 0 | Yes |
| 200 | 400 | 400 | 100% | 1 ms | 3 ms | 0 | Yes |

**Result:** SyncPad sustained at least **200 concurrent collaborative sessions
(400 WebSocket clients)** and **400 acknowledged operations per second**, with
zero errors, disconnects, or convergence failures. Every configured step was
stable, so 200 sessions is a verified lower bound, not the server's failure
point or maximum capacity.

The original artifact did not capture VPS hardware or the deployed image digest.
The workflow now records those fields for subsequent runs.

## Public HTTPS/WSS latency

No result is published yet. The capacity artifact was downloaded successfully,
but the corresponding `public-latency.jsonl` artifact is not available locally.
This section will be completed from three valid 60-second workflow runs; the
reported value will be the median of their p50 and p95 remote-apply latencies.
