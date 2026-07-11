//! Convergence fuzz harness (spec §11) — the most important test in the
//! repository. N simulated clients apply random operations with random
//! interleavings and deliberately delayed event processing; when the dust
//! settles, every client's view and the server's authoritative content must
//! be byte-identical. Zero divergence, every seed, every time.
//!
//! Each simulated client is a faithful implementation of the ot.js client
//! state machine (Synchronized / AwaitingConfirm / AwaitingWithBuffer) —
//! the same semantics the TypeScript editor client mirrors.
//!
//! Scenarios are seeded and reproducible. A failure names its seed; re-run
//! it exactly and keep it as a regression case. Tune the workload locally:
//! `SYNCPAD_FUZZ_SEEDS=500 SYNCPAD_FUZZ_ROUNDS=400 cargo test -p
//! syncpad-server --test fuzz_convergence`.

use operational_transform::OperationSeq;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use syncpad_server::doc::{self, Doc, DocHandle, Joined, Recipients};
use syncpad_server::protocol::ServerMessage;
use tokio::sync::broadcast::error::TryRecvError;

/// Insert alphabet includes multibyte scalars so char-offset handling is
/// exercised end to end (the OT algebra counts Unicode scalar values).
const INSERT_CHARS: &[char] = &[
    'a', 'b', 'c', 'x', 'y', 'z', ' ', '\n', 'é', 'λ', '中', '🦀',
];

/// Documents longer than this bias the generator toward deletes.
const MAX_DOC_CHARS: u64 = 100;

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// A random operation valid against `content` (char counts, not bytes).
fn random_op(content: &str, rng: &mut StdRng) -> OperationSeq {
    let len = content.chars().count() as u64;
    let mut op = OperationSeq::default();

    let delete = len > 0 && (len > MAX_DOC_CHARS || rng.random_bool(0.4));
    if delete {
        let offset = rng.random_range(0..len);
        let count = rng.random_range(1..=(len - offset).min(8));
        op.retain(offset);
        op.delete(count);
        op.retain(len - offset - count);
    } else {
        let offset = rng.random_range(0..=len);
        let text: String = (0..rng.random_range(1..=4))
            .map(|_| INSERT_CHARS[rng.random_range(0..INSERT_CHARS.len())])
            .collect();
        op.retain(offset);
        op.insert(&text);
        op.retain(len - offset);
    }
    op
}

/// The ot.js client state machine: local edits apply immediately, one
/// operation is in flight at a time, further edits compose into a buffer,
/// and incoming server operations are transformed through the pending
/// pipeline before touching the local view.
struct SimClient {
    self_id: String,
    content: String,
    revision: u64,
    outstanding: Option<OperationSeq>,
    buffer: Option<OperationSeq>,
    events: tokio::sync::broadcast::Receiver<doc::Envelope>,
}

impl SimClient {
    fn new(joined: Joined) -> Self {
        Self {
            self_id: joined.self_id,
            content: joined.content,
            revision: joined.revision,
            outstanding: None,
            buffer: None,
            events: joined.events,
        }
    }

    fn settled(&self) -> bool {
        self.outstanding.is_none() && self.buffer.is_none()
    }

    /// Apply an edit to the local view and send or buffer it.
    async fn submit(&mut self, handle: &DocHandle, op: OperationSeq, seed: u64) {
        self.content = op
            .apply(&self.content)
            .unwrap_or_else(|error| panic!("seed {seed}: local apply failed: {error}"));

        if self.outstanding.is_none() {
            self.send(handle, &op).await;
            self.outstanding = Some(op);
        } else if let Some(buffer) = self.buffer.take() {
            let composed = buffer
                .compose(&op)
                .unwrap_or_else(|error| panic!("seed {seed}: buffer compose failed: {error}"));
            self.buffer = Some(composed);
        } else {
            self.buffer = Some(op);
        }
    }

    async fn local_edit(&mut self, handle: &DocHandle, rng: &mut StdRng, seed: u64) {
        let op = random_op(&self.content, rng);
        self.submit(handle, op, seed).await;
    }

    async fn send(&self, handle: &DocHandle, op: &OperationSeq) {
        handle
            .op(
                self.self_id.clone(),
                self.revision,
                serde_json::to_value(op).expect("op serializes"),
                0,
            )
            .await;
    }

    /// Deliverability filter — identical to the connection layer's.
    fn deliverable(&self, recipients: &Recipients) -> bool {
        match recipients {
            Recipients::All => true,
            Recipients::Except(id) => *id != self.self_id,
            Recipients::Only(id) => *id == self.self_id,
        }
    }

    /// Process everything currently pending; returns how many messages were
    /// handled so the drain loop can detect quiescence.
    async fn process_pending(&mut self, handle: &DocHandle, seed: u64) -> usize {
        let mut processed = 0;
        loop {
            match self.events.try_recv() {
                Ok(envelope) => {
                    if !self.deliverable(&envelope.recipients) {
                        continue;
                    }
                    self.handle_message(handle, envelope.msg, seed).await;
                    processed += 1;
                }
                Err(TryRecvError::Empty) => return processed,
                Err(TryRecvError::Lagged(missed)) => panic!(
                    "seed {seed}: client {} lagged {missed} events; \
                     scenario outgrew the event buffer",
                    self.self_id
                ),
                Err(TryRecvError::Closed) => {
                    panic!(
                        "seed {seed}: event channel closed under client {}",
                        self.self_id
                    )
                }
            }
        }
    }

    async fn handle_message(&mut self, handle: &DocHandle, msg: ServerMessage, seed: u64) {
        match msg {
            ServerMessage::Ack { revision } => {
                assert_eq!(
                    revision,
                    self.revision + 1,
                    "seed {seed}: ack out of order for client {}",
                    self.self_id
                );
                assert!(
                    self.outstanding.is_some(),
                    "seed {seed}: ack with nothing outstanding for client {}",
                    self.self_id
                );
                self.revision = revision;
                match self.buffer.take() {
                    Some(buffer) => {
                        self.send(handle, &buffer).await;
                        self.outstanding = Some(buffer);
                    }
                    None => self.outstanding = None,
                }
            }
            ServerMessage::Op { revision, ops, .. } => {
                assert_eq!(
                    revision,
                    self.revision + 1,
                    "seed {seed}: op out of order for client {}",
                    self.self_id
                );
                let mut op: OperationSeq = serde_json::from_value(ops).unwrap_or_else(|error| {
                    panic!("seed {seed}: broadcast op unparseable: {error}")
                });

                if let Some(outstanding) = self.outstanding.take() {
                    let (outstanding_prime, op_prime) =
                        outstanding.transform(&op).unwrap_or_else(|error| {
                            panic!("seed {seed}: transform against outstanding failed: {error}")
                        });
                    self.outstanding = Some(outstanding_prime);
                    op = op_prime;
                }
                if let Some(buffer) = self.buffer.take() {
                    let (buffer_prime, op_prime) = buffer.transform(&op).unwrap_or_else(|error| {
                        panic!("seed {seed}: transform against buffer failed: {error}")
                    });
                    self.buffer = Some(buffer_prime);
                    op = op_prime;
                }

                self.content = op
                    .apply(&self.content)
                    .unwrap_or_else(|error| panic!("seed {seed}: remote apply failed: {error}"));
                self.revision = revision;
            }
            // Presence and the rest of the protocol are irrelevant to
            // convergence.
            _ => {}
        }
    }
}

/// Pump events until every client is settled and nothing new arrives.
async fn drain_to_quiescence(clients: &mut [SimClient], handle: &DocHandle, seed: u64) {
    let mut quiet_passes = 0;
    for _ in 0..10_000 {
        let mut processed = 0;
        for client in clients.iter_mut() {
            processed += client.process_pending(handle, seed).await;
        }
        if processed == 0 && clients.iter().all(SimClient::settled) {
            quiet_passes += 1;
            if quiet_passes >= 3 {
                return;
            }
        } else {
            quiet_passes = 0;
        }
        tokio::task::yield_now().await;
    }
    panic!("seed {seed}: scenario did not quiesce");
}

async fn run_scenario(seed: u64, rounds: u64) {
    let mut rng = StdRng::seed_from_u64(seed);
    let handle = doc::spawn(Doc::default());

    let client_count = rng.random_range(2..=5);
    let mut clients = Vec::with_capacity(client_count);
    for _ in 0..client_count {
        let joined = handle.join().await.expect("join");
        clients.push(SimClient::new(joined));
    }

    for round in 0..rounds {
        let idx = rng.random_range(0..clients.len());
        match rng.random_range(0..100) {
            0..45 => clients[idx].local_edit(&handle, &mut rng, seed).await,
            45..90 => {
                clients[idx].process_pending(&handle, seed).await;
            }
            // Otherwise idle: stale-base concurrency comes from clients
            // editing before they have drained pending events.
            _ => {}
        }

        // Periodic full drain keeps slow readers inside the event buffer
        // without removing the concurrency the fuzz exists to create.
        if round % 50 == 49 {
            for client in clients.iter_mut() {
                client.process_pending(&handle, seed).await;
            }
        }
        tokio::task::yield_now().await;
    }

    drain_to_quiescence(&mut clients, &handle, seed).await;

    let authoritative = handle.join().await.expect("final join");
    for client in &clients {
        assert_eq!(
            client.content, authoritative.content,
            "seed {seed}: client {} diverged from the server",
            client.self_id
        );
        assert_eq!(
            client.revision, authoritative.revision,
            "seed {seed}: client {} revision mismatch",
            client.self_id
        );
    }
}

/// The harness itself: fixed seeds so CI is deterministic, tunable via
/// environment for longer local runs.
#[tokio::test]
async fn fuzz_convergence_fixed_seeds() {
    let seeds = env_u64("SYNCPAD_FUZZ_SEEDS", 50);
    let rounds = env_u64("SYNCPAD_FUZZ_ROUNDS", 200);
    for seed in 0..seeds {
        run_scenario(seed, rounds).await;
    }
}

/// Validates the simulated client against a known concurrent case before
/// trusting it as the fuzz oracle: from "ab", A inserts "x" at 0 and B
/// concurrently inserts "y" at 2 — everything must converge on "xaby".
#[tokio::test]
async fn sim_clients_replay_known_concurrent_case() {
    let seed = u64::MAX; // only used for failure messages here
    let handle = doc::spawn(Doc {
        content: "ab".to_string(),
        revision: 0,
        language: "plaintext".to_string(),
    });

    let mut a = SimClient::new(handle.join().await.expect("join a"));
    let mut b = SimClient::new(handle.join().await.expect("join b"));

    let mut a_op = OperationSeq::default();
    a_op.insert("x");
    a_op.retain(2);
    a.submit(&handle, a_op, seed).await;

    // B has not processed A's edit — a genuinely stale-base operation.
    let mut b_op = OperationSeq::default();
    b_op.retain(2);
    b_op.insert("y");
    b.submit(&handle, b_op, seed).await;

    let mut clients = [a, b];
    drain_to_quiescence(&mut clients, &handle, seed).await;

    let authoritative = handle.join().await.expect("final join");
    assert_eq!(authoritative.content, "xaby");
    assert_eq!(authoritative.revision, 2);
    for client in &clients {
        assert_eq!(client.content, "xaby");
        assert_eq!(client.revision, 2);
    }
}
