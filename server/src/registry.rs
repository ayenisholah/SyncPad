//! In-memory document registry (spec §5.2). Documents are created on demand
//! and identified by an unguessable slug — the slug is the only capability
//! (spec NFR6). The registry maps ids to running document tasks; lazy
//! snapshot hydration replaces the plain create-on-demand path once
//! persistence lands.

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rand::Rng;

use crate::doc::{self, Doc, DocHandle};

/// 32-character alphabet without visually ambiguous letters (i, l, o, u),
/// after Crockford base32. 8 characters give 32^8 ≈ 1.1 × 10^12 slugs.
pub const SLUG_ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// Length of a document slug.
pub const SLUG_LEN: usize = 8;

/// A random identifier drawn from [`SLUG_ALPHABET`].
pub fn random_id(len: usize) -> String {
    let mut rng = rand::rng();
    (0..len)
        .map(|_| {
            let idx = rng.random_range(0..SLUG_ALPHABET.len());
            SLUG_ALPHABET[idx] as char
        })
        .collect()
}

/// A new document slug.
pub fn generate_slug() -> String {
    random_id(SLUG_LEN)
}

/// docId → handle of the owning task, created on demand.
#[derive(Debug, Default)]
pub struct Registry {
    docs: DashMap<String, DocHandle>,
}

impl Registry {
    /// Create a fresh empty document and return its slug. Retries on the
    /// (astronomically unlikely) slug collision rather than overwriting.
    pub fn create(&self) -> String {
        loop {
            let slug = generate_slug();
            if let Entry::Vacant(entry) = self.docs.entry(slug.clone()) {
                entry.insert(doc::spawn(Doc::default()));
                return slug;
            }
        }
    }

    /// Handle to a document's task, spawning an empty document if the id is
    /// unknown (spec §6.4: unknown ids are treated as new documents until
    /// snapshot hydration exists).
    pub fn handle(&self, id: &str) -> DocHandle {
        self.docs
            .entry(id.to_string())
            .or_insert_with(|| doc::spawn(Doc::default()))
            .value()
            .clone()
    }

    /// Whether a document with this id currently exists.
    pub fn contains(&self, id: &str) -> bool {
        self.docs.contains_key(id)
    }

    /// Number of live documents.
    pub fn doc_count(&self) -> usize {
        self.docs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn slug_has_expected_length_and_alphabet() {
        for _ in 0..1000 {
            let slug = generate_slug();
            assert_eq!(slug.len(), SLUG_LEN);
            assert!(slug.bytes().all(|b| SLUG_ALPHABET.contains(&b)));
        }
    }

    #[test]
    fn slug_alphabet_is_32_unique_characters() {
        let unique: HashSet<u8> = SLUG_ALPHABET.iter().copied().collect();
        assert_eq!(unique.len(), 32);
        for ambiguous in b"ilou" {
            assert!(!SLUG_ALPHABET.contains(ambiguous));
        }
    }

    #[test]
    fn slugs_do_not_collide_in_a_small_sample() {
        let sample: HashSet<String> = (0..10_000).map(|_| generate_slug()).collect();
        assert_eq!(sample.len(), 10_000);
    }

    #[tokio::test]
    async fn create_registers_a_live_document() {
        let registry = Registry::default();
        let slug = registry.create();
        assert!(registry.contains(&slug));
        assert_eq!(registry.doc_count(), 1);

        // The spawned task serves fresh-document state.
        let joined = registry.handle(&slug).join().await.expect("join");
        assert_eq!(joined.revision, 0);
        assert_eq!(joined.content, "");
        assert_eq!(joined.language, "plaintext");
    }

    #[tokio::test]
    async fn handle_makes_unknown_ids_live() {
        let registry = Registry::default();
        assert!(!registry.contains("x7k2p9q1"));

        let joined = registry.handle("x7k2p9q1").join().await.expect("join");
        assert_eq!(joined.revision, 0);
        assert!(registry.contains("x7k2p9q1"));
    }

    #[tokio::test]
    async fn handle_returns_the_same_document() {
        let registry = Registry::default();
        let slug = registry.create();

        let first = registry.handle(&slug).join().await.expect("first join");
        let second = registry.handle(&slug).join().await.expect("second join");

        // Same task: the second join sees the first participant.
        assert_eq!(second.participants.len(), 1);
        assert_eq!(second.participants[0].id, first.self_id);
        assert_eq!(registry.doc_count(), 1);
    }
}
