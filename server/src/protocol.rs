//! Wire protocol for the sync WebSocket (spec §6.2). All messages are JSON
//! objects tagged by `type`, with camelCase field names.
//!
//! `ops` payloads are kept as raw JSON values until the operational-transform
//! integration lands: the crate's own JSON form (`["retain",n]`-style
//! primitives) is the wire format, and the server should not impose a second
//! schema on top of it.

use serde::{Deserialize, Serialize};

/// A participant as seen by other clients: server-assigned name and color.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    pub id: String,
    pub name: String,
    pub color: String,
}

/// A selection range in absolute character offsets (anchor may be after head).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub anchor: u64,
    pub head: u64,
}

/// Messages a client may send to the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ClientMessage {
    Op {
        base_revision: u64,
        ops: serde_json::Value,
        sent_at: u64,
    },
    Cursor {
        position: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selection: Option<Selection>,
    },
    SetLanguage {
        language: String,
    },
    Ping {
        t0: u64,
    },
}

/// Messages the server may send to a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ServerMessage {
    Init {
        revision: u64,
        content: String,
        language: String,
        participants: Vec<Participant>,
        self_id: String,
    },
    Op {
        revision: u64,
        ops: serde_json::Value,
        author_id: String,
        sent_at: u64,
    },
    Ack {
        revision: u64,
    },
    Cursor {
        author_id: String,
        position: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selection: Option<Selection>,
    },
    Presence {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        joined: Option<Participant>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        left: Option<String>,
    },
    Language {
        language: String,
    },
    Pong {
        t0: u64,
        t1: u64,
    },
    Resync,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn round_trip_client(msg: &ClientMessage) -> ClientMessage {
        let text = serde_json::to_string(msg).expect("serialize");
        serde_json::from_str(&text).expect("deserialize")
    }

    fn round_trip_server(msg: &ServerMessage) -> ServerMessage {
        let text = serde_json::to_string(msg).expect("serialize");
        serde_json::from_str(&text).expect("deserialize")
    }

    #[test]
    fn client_op_round_trips_and_uses_wire_names() {
        let msg = ClientMessage::Op {
            base_revision: 12,
            ops: json!([{ "retain": 10 }, { "insert": "x" }, { "retain": 4 }]),
            sent_at: 1_700_000_000_123,
        };
        assert_eq!(round_trip_client(&msg), msg);

        let value: serde_json::Value = serde_json::to_value(&msg).expect("to_value");
        assert_eq!(value["type"], "op");
        assert_eq!(value["baseRevision"], 12);
        assert_eq!(value["sentAt"], 1_700_000_000_123u64);
    }

    #[test]
    fn client_cursor_selection_is_optional() {
        let bare = ClientMessage::Cursor {
            position: 42,
            selection: None,
        };
        let value = serde_json::to_value(&bare).expect("to_value");
        assert!(value.get("selection").is_none());
        assert_eq!(round_trip_client(&bare), bare);

        let with_selection = ClientMessage::Cursor {
            position: 42,
            selection: Some(Selection {
                anchor: 40,
                head: 55,
            }),
        };
        assert_eq!(round_trip_client(&with_selection), with_selection);
    }

    #[test]
    fn client_set_language_and_ping_round_trip() {
        let lang = ClientMessage::SetLanguage {
            language: "rust".to_string(),
        };
        let value = serde_json::to_value(&lang).expect("to_value");
        assert_eq!(value["type"], "setLanguage");
        assert_eq!(round_trip_client(&lang), lang);

        let ping = ClientMessage::Ping { t0: 123 };
        assert_eq!(round_trip_client(&ping), ping);
    }

    #[test]
    fn server_init_round_trips_and_uses_wire_names() {
        let msg = ServerMessage::Init {
            revision: 0,
            content: String::new(),
            language: "plaintext".to_string(),
            participants: vec![Participant {
                id: "abc123".to_string(),
                name: "calm-fox".to_string(),
                color: "#a78bfa".to_string(),
            }],
            self_id: "def456".to_string(),
        };
        assert_eq!(round_trip_server(&msg), msg);

        let value = serde_json::to_value(&msg).expect("to_value");
        assert_eq!(value["type"], "init");
        assert_eq!(value["selfId"], "def456");
        assert_eq!(value["participants"][0]["name"], "calm-fox");
    }

    #[test]
    fn server_op_ack_and_resync_round_trip() {
        let op = ServerMessage::Op {
            revision: 15,
            ops: json!([{ "retain": 3 }, { "delete": 1 }]),
            author_id: "abc123".to_string(),
            sent_at: 99,
        };
        let value = serde_json::to_value(&op).expect("to_value");
        assert_eq!(value["authorId"], "abc123");
        assert_eq!(round_trip_server(&op), op);

        let ack = ServerMessage::Ack { revision: 15 };
        assert_eq!(round_trip_server(&ack), ack);

        let resync = ServerMessage::Resync;
        let value = serde_json::to_value(&resync).expect("to_value");
        assert_eq!(value, json!({ "type": "resync" }));
        assert_eq!(round_trip_server(&resync), resync);
    }

    #[test]
    fn server_presence_deltas_round_trip() {
        let joined = ServerMessage::Presence {
            joined: Some(Participant {
                id: "abc123".to_string(),
                name: "swift-crane".to_string(),
                color: "#2dd4bf".to_string(),
            }),
            left: None,
        };
        assert_eq!(round_trip_server(&joined), joined);

        let left = ServerMessage::Presence {
            joined: None,
            left: Some("abc123".to_string()),
        };
        let value = serde_json::to_value(&left).expect("to_value");
        assert!(value.get("joined").is_none());
        assert_eq!(round_trip_server(&left), left);
    }

    #[test]
    fn unknown_message_type_is_rejected() {
        let result: Result<ClientMessage, _> = serde_json::from_str(r#"{"type":"shutdown"}"#);
        assert!(result.is_err());
    }
}
