//! Identifier generation.

use feast_application::ports::IdGenerator;
use rand::Rng;
use uuid::Uuid;

/// Unambiguous characters for codes people read aloud (no 0/O, 1/I).
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Time-ordered UUIDv7 ids (`bk_0192f…`): sortable, index-friendly, safe to generate on any node.
#[derive(Debug, Default, Clone)]
pub struct UuidIds;

impl IdGenerator for UuidIds {
    fn new_id(&self, prefix: &str) -> String {
        format!("{prefix}_{}", Uuid::now_v7().simple())
    }

    fn short_code(&self, len: usize) -> String {
        let mut rng = rand::thread_rng();
        (0..len).map(|_| CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())] as char).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_prefixed_and_sortable() {
        let g = UuidIds;
        let a = g.new_id("bk");
        let b = g.new_id("bk");
        assert!(a.starts_with("bk_") && a.len() == 35);
        assert!(a < b, "v7 ids sort by creation time");
        assert_eq!(g.short_code(6).len(), 6);
    }
}
