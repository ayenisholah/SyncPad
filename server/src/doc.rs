//! Per-document task (spec §5.2, §6.6). Each live document is owned by one
//! tokio task: connections send commands over an mpsc channel and receive
//! events over a broadcast channel, so all document mutation is
//! single-threaded with no locks around document state.

use std::collections::HashMap;

use rand::Rng;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::protocol::{Participant, ServerMessage};
use crate::registry::random_id;

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

/// A broadcast event, optionally excluding one connection (the author —
/// senders never receive their own messages back).
#[derive(Debug, Clone)]
pub struct Envelope {
    pub exclude: Option<String>,
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
    Join { reply: oneshot::Sender<Joined> },
    Leave { conn_id: String },
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
    doc: Doc,
    mut commands: mpsc::Receiver<DocCommand>,
    events: broadcast::Sender<Envelope>,
) {
    let mut presence: HashMap<String, Participant> = HashMap::new();

    while let Some(command) = commands.recv().await {
        match command {
            DocCommand::Join { reply } => {
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
                    exclude: Some(self_id),
                    msg: ServerMessage::Presence {
                        joined: Some(participant),
                        left: None,
                    },
                });
                let _ = reply.send(joined);
            }
            DocCommand::Leave { conn_id } => {
                if presence.remove(&conn_id).is_some() {
                    let _ = events.send(Envelope {
                        exclude: None,
                        msg: ServerMessage::Presence {
                            joined: None,
                            left: Some(conn_id),
                        },
                    });
                }
            }
        }
    }
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

        // The joiner's own event is delivered but marked excluded — the
        // connection layer uses that to avoid echoing to the author.
        let own = first.events.recv().await.expect("own join event");
        assert_eq!(own.exclude.as_deref(), Some(first.self_id.as_str()));

        let second = handle.join().await.expect("second join");
        let event = first.events.recv().await.expect("peer join event");
        assert_eq!(event.exclude.as_deref(), Some(second.self_id.as_str()));
        match event.msg {
            ServerMessage::Presence {
                joined: Some(participant),
                left: None,
            } => assert_eq!(participant.id, second.self_id),
            other => panic!("expected join presence, got {other:?}"),
        }

        handle.leave(second.self_id.clone()).await;
        let event = first.events.recv().await.expect("leave event");
        assert_eq!(event.exclude, None);
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
