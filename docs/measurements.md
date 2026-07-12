# Production Measurements

Measured values are reported separately for the public network path and the
server's loopback path. Loopback results describe server capacity and processing
latency; they are not internet latency claims.

## Server capacity

Confirmed on 2026-07-12 at 16:42:21 UTC by
[workflow run 29200610594](https://github.com/ayenisholah/SyncPad/actions/runs/29200610594)
at commit `791bd1e`. The harness ran on the production VPS against
`http://127.0.0.1:8090`, bypassing nginx and TLS but exercising the deployed
SyncPad server. Synthetic source addresses were used only on loopback so the
production limit of 10 active documents per public IP remained enabled.

The VPS had 4 CPU cores and 7.1 GiB RAM (5.5 GiB available at measurement
time), running 64-bit Ubuntu kernel `7.0.0-22-generic`. The deployed container
was `ghcr.io/ayenisholah/syncpad:edge`, image ID `14b36d3b2fc1`, with an
11.1 MB image size.

Each collaborative session had two WebSocket clients sharing one document.
Every session submitted one operation every 500 ms. Load increased by 20
sessions per step and each step ran for 30 seconds. A stable step required at
least 99% acknowledgements, no unexpected disconnects, no convergence failures,
and p95 remote-apply latency below 250 ms.

| Sessions | Clients | Acked ops/s | Ack rate | p50 | p95 | Failures | Stable |
|---:|---:|---:|---:|---:|---:|---:|:---:|
| 20 | 40 | 40 | 100% | 1 ms | 3 ms | 0 | Yes |
| 40 | 80 | 80 | 100% | 1 ms | 4 ms | 0 | Yes |
| 60 | 120 | 120 | 100% | 1 ms | 3 ms | 0 | Yes |
| 80 | 160 | 160 | 100% | 1 ms | 3 ms | 0 | Yes |
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

This run confirms the earlier capacity result with reproducible workflow,
host, and image metadata. Swap was disabled during the measurement.

## Public HTTPS/WSS latency

Measured on 2026-07-12 from the Windows development host in Africa/Lagos against
`https://syncpad.sholaayeni.xyz`. Each run used one document with two WebSocket
clients for 60 seconds, submitting one operation every 500 ms through the full
HTTPS/WSS, nginx, and public-network path. Sender and receiver ran in the same
process, so `Date.now()` timestamps required no cross-machine clock correction.

| Run | Sent | Acknowledged | Received | Ack rate | p50 | p95 | Failures |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 59 | 59 | 59 | 100% | 349 ms | 1,434 ms | 0 |
| 2 | 59 | 59 | 59 | 100% | 342 ms | 1,506 ms | 0 |
| 3 | 57 | 57 | 57 | 100% | 356 ms | 1,220 ms | 0 |

**Result:** the median public remote-apply latency was **p50 349 ms** and
**p95 1,434 ms**. All three runs acknowledged every operation and ended with
byte-identical clients, with zero errors, disconnects, or convergence failures.
The measured latency does not meet the original p50 < 50 ms target; these are
the observed production values, not an estimate.

During preliminary measurement, the harness's fixed 600 ms settlement delay
could compare replicas before a high-latency broadcast arrived. The harness was
corrected to wait until both clients are idle at the same revision with identical
content before validating convergence. Only the three corrected runs above are
published.
