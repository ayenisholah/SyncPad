//! In-memory document registry (spec §5.2). Documents are created on demand
//! and identified by an unguessable slug — the slug is the only capability
//! (spec NFR6). Lazy snapshot hydration replaces the plain create-on-demand
//! path once persistence lands.

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rand::Rng;

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

/// The authoritative state of one document. Owned by a per-document task
/// once the doc task lands; at this stage the registry holds it directly.
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

/// docId → document state, created on demand.
#[derive(Debug, Default)]
pub struct Registry {
    docs: DashMap<String, Doc>,
}

impl Registry {
    /// Create a fresh empty document and return its slug. Retries on the
    /// (astronomically unlikely) slug collision rather than overwriting.
    pub fn create(&self) -> String {
        loop {
            let slug = generate_slug();
            if let Entry::Vacant(entry) = self.docs.entry(slug.clone()) {
                entry.insert(Doc::default());
                return slug;
            }
        }
    }

    /// Current state of a document, creating an empty one if the id is
    /// unknown (spec §6.4: unknown ids are treated as new documents until
    /// snapshot hydration exists).
    pub fn get_or_create(&self, id: &str) -> Doc {
        self.docs.entry(id.to_string()).or_default().value().clone()
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

    #[test]
    fn create_registers_an_empty_document() {
        let registry = Registry::default();
        let slug = registry.create();
        assert!(registry.contains(&slug));
        assert_eq!(registry.doc_count(), 1);

        let doc = registry.get_or_create(&slug);
        assert_eq!(doc.revision, 0);
        assert_eq!(doc.content, "");
        assert_eq!(doc.language, "plaintext");
    }

    #[test]
    fn get_or_create_makes_unknown_ids_live() {
        let registry = Registry::default();
        assert!(!registry.contains("x7k2p9q1"));
        let doc = registry.get_or_create("x7k2p9q1");
        assert_eq!(doc.revision, 0);
        assert!(registry.contains("x7k2p9q1"));
    }
}
