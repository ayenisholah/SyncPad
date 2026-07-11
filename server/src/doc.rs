//! Per-document task (spec §5.2, §6.1, §6.6). Each live document is owned
//! by one tokio task: connections send commands over an mpsc channel and
//! receive events over a broadcast channel, so all document mutation is
//! single-threaded with no locks around OT state.
//!
//! The transform algebra comes from the `operational-transform` crate
//! (D-003); this module owns the protocol around it — revision ordering,
//! the replay window, acks, and recovery.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use operational_transform::OperationSeq;
use rand::Rng;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::protocol::{Participant, ServerMessage};
use crate::registry::random_id;
use crate::snapshot::{self, Snapshot};

const COMMAND_BUFFER: usize = 64;
const EVENT_BUFFER: usize = 256;

const ADJECTIVES: &[&str] = &[
    "brave", "calm", "swift", "quiet", "bright", "bold", "gentle", "keen", "lively", "merry",
    "noble", "proud", "spry", "sunny", "wise", "witty",
];

const ANIMALS: &[&str] = &[
    "otter", "fox", "crane", "lynx", "heron", "badger", "raven", "hare", "wren", "stoat", "ibis",
    "newt", "mole", "swan", "finch", "seal",
];

/// Cursor/presence colors; the first entries match the app accent palette.
const PALETTE: &[&str] = &[
    "#a78bfa", "#f59e0b", "#2dd4bf", "#f472b6", "#60a5fa", "#a3e635", "#fb923c", "#e879f9",
];

/// The authoritative state of one document, owned by its task.
#[derive(Debug, Clone)]
pub struct Doc {
    pub content: String,
    pub revision: u64,
    pub language: String,
}

impl Default for Doc {
    fn default() -> Self {
        Self {
            content: String::new(),
            revision: 0,
            language: "plaintext".to_string(),
        }
    }
}

/// Who a broadcast event is for. Everything flows through the one broadcast
/// channel — including single-recipient acks and resyncs — so every
/// connection observes acks and ops in strict revision order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipients {
    All,
    Except(String),
    Only(String),
}

/// A broadcast event; the connection layer delivers it according to
/// `recipients`.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub recipients: Recipients,
    pub msg: ServerMessage,
}

/// What a connection receives back when it joins a document: the state to
/// build `init` from (roster excludes the joiner itself) and the event
/// subscription, created inside the task so no later event can be missed.
#[derive(Debug)]
pub struct Joined {
    pub self_id: String,
    pub revision: u64,
    pub content: String,
    pub language: String,
    pub participants: Vec<Participant>,
    pub events: broadcast::Receiver<Envelope>,
}

#[derive(Debug)]
enum DocCommand {
    Join {
        reply: oneshot::Sender<Joined>,
    },
    Leave {
        conn_id: String,
    },
    Op {
        conn_id: String,
        base_revision: u64,
        ops: serde_json::Value,
        sent_at: u64,
    },
    Snapshot {
        reply: oneshot::Sender<Option<Snapshot>>,
    },
    Reap {
        idle_after: Duration,
        reply: oneshot::Sender<bool>,
    },
}

/// Cheap-to-clone handle for sending commands to a document task.
#[derive(Debug, Clone)]
pub struct DocHandle {
    tx: mpsc::Sender<DocCommand>,
}

impl DocHandle {
    /// Join the document. `None` means the task is gone (e.g. panicked);
    /// callers should close the connection and let the client reconnect.
    pub async fn join(&self) -> Option<Joined> {
        let (reply, response) = oneshot::channel();
        self.tx.send(DocCommand::Join { reply }).await.ok()?;
        response.await.ok()
    }

    pub async fn leave(&self, conn_id: String) {
        let _ = self.tx.send(DocCommand::Leave { conn_id }).await;
    }

    /// Submit an operation based on `base_revision` (spec FR3). The outcome
    /// arrives as an `ack` (accepted), or `resync` + `init` (rejected).
    pub async fn op(
        &self,
        conn_id: String,
        base_revision: u64,
        ops: serde_json::Value,
        sent_at: u64,
    ) {
        let _ = self
            .tx
            .send(DocCommand::Op {
                conn_id,
                base_revision,
                ops,
                sent_at,
            })
            .await;
    }

    /// Take a snapshot of the document if it has unsaved changes (spec §6.4).
    /// `Some` clears the task's dirty flag and truncates its replay window;
    /// `None` means nothing changed since the last snapshot, or the task is
    /// gone. See the snapshot service in [`crate::snapshot`].
    pub async fn snapshot(&self) -> Option<Snapshot> {
        let (reply, response) = oneshot::channel();
        self.tx.send(DocCommand::Snapshot { reply }).await.ok()?;
        response.await.ok().flatten()
    }

    /// Whether the document is idle enough to expire (spec FR8): no connected
    /// participants and no activity within `idle_after`. A task that is already
    /// gone answers `false` — there is nothing to reap.
    pub async fn should_reap(&self, idle_after: Duration) -> bool {
        let (reply, response) = oneshot::channel();
        if self
            .tx
            .send(DocCommand::Reap { idle_after, reply })
            .await
            .is_err()
        {
            return false;
        }
        response.await.unwrap_or(false)
    }
}

/// Spawn the task owning one document and return its handle. The task ends
/// when every handle is dropped; idle-document expiry is a separate concern
/// (spec FR8) and not handled here.
pub fn spawn(doc: Doc) -> DocHandle {
    let (tx, rx) = mpsc::channel(COMMAND_BUFFER);
    let (events, _) = broadcast::channel(EVENT_BUFFER);
    tokio::spawn(run(doc, rx, events));
    DocHandle { tx }
}

async fn run(
    mut doc: Doc,
    mut commands: mpsc::Receiver<DocCommand>,
    events: broadcast::Sender<Envelope>,
) {
    let mut presence: HashMap<String, Participant> = HashMap::new();
    // Ops accepted since the last snapshot; entry i is the op that produced
    // revision `log_start + i + 1`. A snapshot truncates this to a replay
    // window rooted at the snapshot revision (spec §6.4, FR9/FR10, NFR5).
    let mut log: Vec<OperationSeq> = Vec::new();
    // Set on every accepted op; cleared when a snapshot is taken. A hydrated
    // document starts clean — its content already matches its snapshot.
    let mut dirty = false;
    // Last real interaction (join/leave/accepted op), for the idle reaper
    // (spec FR8). Snapshot and reap polls do not count as activity.
    let mut last_active = Instant::now();

    while let Some(command) = commands.recv().await {
        match command {
            DocCommand::Join { reply } => {
                last_active = Instant::now();
                let self_id = random_id(16);
                let participant = Participant {
                    id: self_id.clone(),
                    name: assign_name(&presence),
                    color: assign_color(&presence),
                };

                let joined = Joined {
                    self_id: self_id.clone(),
                    revision: doc.revision,
                    content: doc.content.clone(),
                    language: doc.language.clone(),
                    participants: presence.values().cloned().collect(),
                    events: events.subscribe(),
                };

                presence.insert(self_id.clone(), participant.clone());
                let _ = events.send(Envelope {
                    recipients: Recipients::Except(self_id),
                    msg: ServerMessage::Presence {
                        joined: Some(participant),
                        left: None,
                    },
                });
                let _ = reply.send(joined);
            }
            DocCommand::Leave { conn_id } => {
                last_active = Instant::now();
                if presence.remove(&conn_id).is_some() {
                    let _ = events.send(Envelope {
                        recipients: Recipients::All,
                        msg: ServerMessage::Presence {
                            joined: None,
                            left: Some(conn_id),
                        },
                    });
                }
            }
            DocCommand::Op {
                conn_id,
                base_revision,
                ops,
                sent_at,
            } => {
                if handle_op(
                    &mut doc,
                    &mut log,
                    &presence,
                    &events,
                    conn_id,
                    base_revision,
                    ops,
                    sent_at,
                ) {
                    dirty = true;
                    last_active = Instant::now();
                }
            }
            DocCommand::Reap { idle_after, reply } => {
                let idle = presence.is_empty() && last_active.elapsed() >= idle_after;
                let _ = reply.send(idle);
            }
            DocCommand::Snapshot { reply } => {
                let snapshot = if dirty {
                    // Clear dirty and truncate the replay window optimistically,
                    // before the write is confirmed: the content stays safely in
                    // memory, and any accepted op re-marks dirty, so a failed
                    // write costs at most one interval (spec §6.4 tolerance).
                    dirty = false;
                    log.clear();
                    Some(Snapshot {
                        content: doc.content.clone(),
                        revision: doc.revision,
                        language: doc.language.clone(),
                        updated_at: snapshot::now_ms(),
                    })
                } else {
                    None
                };
                let _ = reply.send(snapshot);
            }
        }
    }
}

/// The four-step op algorithm from spec §6.1: validate the replay window,
/// transform against concurrent ops, apply, then ack and broadcast. Every
/// rejection leaves the document untouched and forces the sender to resync.
/// Returns `true` when the op was accepted and mutated the document, so the
/// caller can mark it dirty for the next snapshot.
#[allow(clippy::too_many_arguments)]
fn handle_op(
    doc: &mut Doc,
    log: &mut Vec<OperationSeq>,
    presence: &HashMap<String, Participant>,
    events: &broadcast::Sender<Envelope>,
    conn_id: String,
    base_revision: u64,
    ops: serde_json::Value,
    sent_at: u64,
) -> bool {
    let log_start = doc.revision - log.len() as u64;
    if base_revision > doc.revision || base_revision < log_start {
        tracing::debug!(
            conn_id,
            base_revision,
            revision = doc.revision,
            log_start,
            "op outside the replay window; forcing resync"
        );
        resync(doc, presence, events, &conn_id);
        return false;
    }

    let Ok(mut operation) = serde_json::from_value::<OperationSeq>(ops) else {
        tracing::debug!(conn_id, "unparseable operation; forcing resync");
        resync(doc, presence, events, &conn_id);
        return false;
    };

    for concurrent in &log[(base_revision - log_start) as usize..] {
        match operation.transform(concurrent) {
            Ok((client_prime, _)) => operation = client_prime,
            Err(error) => {
                tracing::debug!(conn_id, %error, "transform failed; forcing resync");
                resync(doc, presence, events, &conn_id);
                return false;
            }
        }
    }

    let content = match operation.apply(&doc.content) {
        Ok(content) => content,
        Err(error) => {
            // A mismatched operation means a broken client; the document
            // must never be corrupted on its behalf.
            tracing::debug!(conn_id, %error, "apply failed; forcing resync");
            resync(doc, presence, events, &conn_id);
            return false;
        }
    };

    doc.content = content;
    log.push(operation.clone());
    doc.revision += 1;

    let Ok(ops_json) = serde_json::to_value(&operation) else {
        // The op was applied; the document changed even though this broadcast
        // could not be serialized. Report it accepted so it is persisted.
        return true;
    };
    let _ = events.send(Envelope {
        recipients: Recipients::Only(conn_id.clone()),
        msg: ServerMessage::Ack {
            revision: doc.revision,
        },
    });
    let _ = events.send(Envelope {
        recipients: Recipients::Except(conn_id.clone()),
        msg: ServerMessage::Op {
            revision: doc.revision,
            ops: ops_json,
            author_id: conn_id,
            sent_at,
        },
    });
    true
}

/// Recovery path (spec §6.2): tell one connection to drop its state, then
/// hand it a fresh `init` with the authoritative document.
fn resync(
    doc: &Doc,
    presence: &HashMap<String, Participant>,
    events: &broadcast::Sender<Envelope>,
    conn_id: &str,
) {
    let _ = events.send(Envelope {
        recipients: Recipients::Only(conn_id.to_string()),
        msg: ServerMessage::Resync,
    });
    let _ = events.send(Envelope {
        recipients: Recipients::Only(conn_id.to_string()),
        msg: ServerMessage::Init {
            revision: doc.revision,
            content: doc.content.clone(),
            language: doc.language.clone(),
            participants: presence
                .values()
                .filter(|p| p.id != conn_id)
                .cloned()
                .collect(),
            self_id: conn_id.to_string(),
        },
    });
}

/// Random adjective-animal name, retrying a few times to keep names unique
/// within a document; falls back to a numbered suffix when the room is
/// crowded enough to exhaust the retries.
fn assign_name(presence: &HashMap<String, Participant>) -> String {
    let mut rng = rand::rng();
    let mut name = String::new();
    for _ in 0..8 {
        name = format!(
            "{}-{}",
            ADJECTIVES[rng.random_range(0..ADJECTIVES.len())],
            ANIMALS[rng.random_range(0..ANIMALS.len())]
        );
        if !presence.values().any(|p| p.name == name) {
            return name;
        }
    }
    format!("{name}-{}", presence.len() + 1)
}

/// Least-used palette color, filling the palette in order before repeating.
fn assign_color(presence: &HashMap<String, Participant>) -> String {
    PALETTE
        .iter()
        .min_by_key(|color| presence.values().filter(|p| p.color == **color).count())
        .unwrap_or(&PALETTE[0])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_from_word_lists(name: &str) -> bool {
        name.split_once('-').is_some_and(|(adjective, animal)| {
            ADJECTIVES.contains(&adjective) && ANIMALS.contains(&animal)
        })
    }

    fn op_json(build: impl FnOnce(&mut OperationSeq)) -> serde_json::Value {
        let mut op = OperationSeq::default();
        build(&mut op);
        serde_json::to_value(&op).expect("op json")
    }

    /// Apply a broadcast `op` message to a client-side view of the content.
    fn apply_broadcast(msg: &ServerMessage, view: &str) -> String {
        match msg {
            ServerMessage::Op { ops, .. } => {
                let op: OperationSeq =
                    serde_json::from_value(ops.clone()).expect("broadcast op parses");
                op.apply(view).expect("broadcast op applies")
            }
            other => panic!("expected op broadcast, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn first_join_sees_fresh_doc_and_empty_roster() {
        let handle = spawn(Doc::default());
        let joined = handle.join().await.expect("join");

        assert_eq!(joined.revision, 0);
        assert_eq!(joined.content, "");
        assert_eq!(joined.language, "plaintext");
        assert!(joined.participants.is_empty());
        assert_eq!(joined.self_id.len(), 16);
    }

    #[tokio::test]
    async fn second_join_sees_first_in_roster() {
        let handle = spawn(Doc::default());
        let first = handle.join().await.expect("first join");
        let second = handle.join().await.expect("second join");

        assert_ne!(first.self_id, second.self_id);
        assert_eq!(second.participants.len(), 1);

        let peer = &second.participants[0];
        assert_eq!(peer.id, first.self_id);
        assert!(is_from_word_lists(&peer.name));
        assert!(PALETTE.contains(&peer.color.as_str()));
    }

    #[tokio::test]
    async fn joins_and_leaves_are_broadcast_with_author_excluded() {
        let handle = spawn(Doc::default());
        let mut first = handle.join().await.expect("first join");

        // The joiner's own event is delivered but addressed away from it —
        // the connection layer uses that to avoid echoing to the author.
        let own = first.events.recv().await.expect("own join event");
        assert_eq!(own.recipients, Recipients::Except(first.self_id.clone()));

        let second = handle.join().await.expect("second join");
        let event = first.events.recv().await.expect("peer join event");
        assert_eq!(event.recipients, Recipients::Except(second.self_id.clone()));
        match event.msg {
            ServerMessage::Presence {
                joined: Some(participant),
                left: None,
            } => assert_eq!(participant.id, second.self_id),
            other => panic!("expected join presence, got {other:?}"),
        }

        handle.leave(second.self_id.clone()).await;
        let event = first.events.recv().await.expect("leave event");
        assert_eq!(event.recipients, Recipients::All);
        match event.msg {
            ServerMessage::Presence {
                joined: None,
                left: Some(id),
            } => assert_eq!(id, second.self_id),
            other => panic!("expected leave presence, got {other:?}"),
        }

        // Roster reflects the departure for the next joiner.
        let third = handle.join().await.expect("third join");
        assert_eq!(third.participants.len(), 1);
        assert_eq!(third.participants[0].id, first.self_id);
    }

    #[tokio::test]
    async fn leaving_twice_broadcasts_once() {
        let handle = spawn(Doc::default());
        let mut first = handle.join().await.expect("first join");
        let _ = first.events.recv().await.expect("own join event");

        let second = handle.join().await.expect("second join");
        let _ = first.events.recv().await.expect("peer join event");

        handle.leave(second.self_id.clone()).await;
        handle.leave(second.self_id.clone()).await;
        let _ = first.events.recv().await.expect("leave event");

        // Only one leave event was sent; the channel is empty again.
        assert!(matches!(
            first.events.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn accepted_op_acks_sender_and_broadcasts_to_peers() {
        let handle = spawn(Doc::default());
        let mut a = handle.join().await.expect("join a");
        let _ = a.events.recv().await.expect("own join");
        let b = handle.join().await.expect("join b");
        let _ = a.events.recv().await.expect("b join");

        handle
            .op(a.self_id.clone(), 0, op_json(|op| op.insert("hello")), 123)
            .await;

        let ack = a.events.recv().await.expect("ack envelope");
        assert_eq!(ack.recipients, Recipients::Only(a.self_id.clone()));
        assert_eq!(ack.msg, ServerMessage::Ack { revision: 1 });

        let broadcast = a.events.recv().await.expect("op envelope");
        assert_eq!(broadcast.recipients, Recipients::Except(a.self_id.clone()));
        match &broadcast.msg {
            ServerMessage::Op {
                revision,
                author_id,
                sent_at,
                ..
            } => {
                assert_eq!(*revision, 1);
                assert_eq!(author_id, &a.self_id);
                assert_eq!(*sent_at, 123);
            }
            other => panic!("expected op broadcast, got {other:?}"),
        }
        assert_eq!(apply_broadcast(&broadcast.msg, ""), "hello");

        // The authoritative state moved; b was a bystander.
        drop(b);
        let late = handle.join().await.expect("late join");
        assert_eq!(late.content, "hello");
        assert_eq!(late.revision, 1);
    }

    #[tokio::test]
    async fn concurrent_ops_converge_through_transform() {
        let handle = spawn(Doc {
            content: "ab".to_string(),
            revision: 0,
            language: "plaintext".to_string(),
        });
        let mut a = handle.join().await.expect("join a");
        let _ = a.events.recv().await.expect("own join");
        let b = handle.join().await.expect("join b");
        let _ = a.events.recv().await.expect("b join");

        // A inserts "x" at offset 0; B concurrently (same base revision)
        // inserts "y" at offset 2. Neither has seen the other's edit.
        handle
            .op(
                a.self_id.clone(),
                0,
                op_json(|op| {
                    op.insert("x");
                    op.retain(2);
                }),
                1,
            )
            .await;
        handle
            .op(
                b.self_id.clone(),
                0,
                op_json(|op| {
                    op.retain(2);
                    op.insert("y");
                }),
                2,
            )
            .await;

        let late = handle.join().await.expect("late join");
        assert_eq!(late.content, "xaby");
        assert_eq!(late.revision, 2);

        // TP1 sanity: A's view after its ack is "xab"; applying the
        // transformed broadcast of B's op must reach the same content the
        // server holds.
        let _ack_a = a.events.recv().await.expect("ack for a");
        let _op_a = a.events.recv().await.expect("a's own broadcast");
        let _ack_b = a.events.recv().await.expect("ack for b");
        let b_broadcast = a.events.recv().await.expect("b's broadcast");
        assert_eq!(
            b_broadcast.recipients,
            Recipients::Except(b.self_id.clone())
        );
        assert_eq!(apply_broadcast(&b_broadcast.msg, "xab"), late.content);
    }

    #[tokio::test]
    async fn op_from_the_future_forces_resync_without_mutation() {
        let handle = spawn(Doc::default());
        let mut a = handle.join().await.expect("join a");
        let _ = a.events.recv().await.expect("own join");

        handle
            .op(a.self_id.clone(), 5, op_json(|op| op.insert("x")), 0)
            .await;

        let resync = a.events.recv().await.expect("resync envelope");
        assert_eq!(resync.recipients, Recipients::Only(a.self_id.clone()));
        assert_eq!(resync.msg, ServerMessage::Resync);

        let init = a.events.recv().await.expect("init envelope");
        assert_eq!(init.recipients, Recipients::Only(a.self_id.clone()));
        match init.msg {
            ServerMessage::Init {
                revision, content, ..
            } => {
                assert_eq!(revision, 0);
                assert_eq!(content, "");
            }
            other => panic!("expected init, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn op_below_the_replay_window_forces_resync() {
        // A document hydrated at revision 5 with an empty log has a replay
        // window starting at 5 — exactly the shape snapshots will produce.
        let handle = spawn(Doc {
            content: "abc".to_string(),
            revision: 5,
            language: "plaintext".to_string(),
        });
        let mut a = handle.join().await.expect("join a");
        let _ = a.events.recv().await.expect("own join");

        handle
            .op(
                a.self_id.clone(),
                3,
                op_json(|op| {
                    op.retain(3);
                    op.insert("!");
                }),
                0,
            )
            .await;

        let resync = a.events.recv().await.expect("resync envelope");
        assert_eq!(resync.msg, ServerMessage::Resync);
        let init = a.events.recv().await.expect("init envelope");
        match init.msg {
            ServerMessage::Init {
                revision, content, ..
            } => {
                assert_eq!(revision, 5);
                assert_eq!(content, "abc");
            }
            other => panic!("expected init, got {other:?}"),
        }

        // The window start itself is still accepted.
        handle
            .op(
                a.self_id.clone(),
                5,
                op_json(|op| {
                    op.retain(3);
                    op.insert("!");
                }),
                0,
            )
            .await;
        let ack = a.events.recv().await.expect("ack envelope");
        assert_eq!(ack.msg, ServerMessage::Ack { revision: 6 });

        let late = handle.join().await.expect("late join");
        assert_eq!(late.content, "abc!");
    }

    #[tokio::test]
    async fn broken_ops_are_rejected_without_corrupting_the_doc() {
        let handle = spawn(Doc {
            content: "abc".to_string(),
            revision: 0,
            language: "plaintext".to_string(),
        });
        let mut a = handle.join().await.expect("join a");
        let _ = a.events.recv().await.expect("own join");

        // Not an operation at all.
        handle
            .op(a.self_id.clone(), 0, serde_json::json!({ "nope": true }), 0)
            .await;
        let resync = a.events.recv().await.expect("resync envelope");
        assert_eq!(resync.msg, ServerMessage::Resync);
        let _init = a.events.recv().await.expect("init envelope");

        // Structurally valid but with the wrong base length.
        handle
            .op(a.self_id.clone(), 0, op_json(|op| op.retain(10)), 0)
            .await;
        let resync = a.events.recv().await.expect("resync envelope");
        assert_eq!(resync.msg, ServerMessage::Resync);
        let _init = a.events.recv().await.expect("init envelope");

        let late = handle.join().await.expect("late join");
        assert_eq!(late.content, "abc");
        assert_eq!(late.revision, 0);
    }

    #[tokio::test]
    async fn empty_document_is_reapable_but_a_joined_one_is_not() {
        let handle = spawn(Doc::default());

        // No connections yet: idle regardless of the window.
        assert!(handle.should_reap(Duration::ZERO).await);

        let joined = handle.join().await.expect("join");
        // A live participant is never reaped, even at a zero idle window.
        assert!(!handle.should_reap(Duration::ZERO).await);

        // After the participant leaves it becomes reapable again.
        handle.leave(joined.self_id).await;
        assert!(handle.should_reap(Duration::ZERO).await);
    }

    #[tokio::test]
    async fn recent_activity_defers_reaping() {
        let handle = spawn(Doc::default());
        // Just spawned: not idle for a full second yet.
        assert!(!handle.should_reap(Duration::from_secs(1)).await);
    }

    #[test]
    fn colors_fill_the_palette_before_repeating() {
        let mut presence = HashMap::new();
        for (i, expected) in PALETTE.iter().enumerate() {
            let color = assign_color(&presence);
            assert_eq!(color, *expected);
            presence.insert(
                format!("conn{i}"),
                Participant {
                    id: format!("conn{i}"),
                    name: format!("name{i}"),
                    color,
                },
            );
        }
        // Palette exhausted: wraps back to the least-used (first) entry.
        assert_eq!(assign_color(&presence), PALETTE[0]);
    }

    #[test]
    fn names_avoid_names_already_present() {
        let mut presence = HashMap::new();
        for i in 0..10 {
            presence.insert(
                format!("conn{i}"),
                Participant {
                    id: format!("conn{i}"),
                    name: format!("{}-{}", ADJECTIVES[i], ANIMALS[i]),
                    color: "#a78bfa".to_string(),
                },
            );
        }

        for _ in 0..100 {
            let name = assign_name(&presence);
            assert!(!presence.values().any(|p| p.name == name));
            assert!(is_from_word_lists(&name));
        }
    }
}
