//! The player's marks on a realm's scanned places: favorites are preferred when quests are made, banned places are never used.
//! Kept apart from the atlas because a rescan replaces the atlas but must not forget the player's choices.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    Favorite,
    Banned,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Marks {
    #[serde(default)]
    pub favorites: BTreeSet<String>,
    #[serde(default)]
    pub banned: BTreeSet<String>,
}

impl Marks {
    /// A shared empty set, for callers (and tests) with nothing marked.
    pub fn none() -> &'static Marks {
        static NONE: Marks = Marks { favorites: BTreeSet::new(), banned: BTreeSet::new() };
        &NONE
    }

    pub fn get(&self, id: &str) -> Mark {
        if self.banned.contains(id) {
            Mark::Banned
        } else if self.favorites.contains(id) {
            Mark::Favorite
        } else {
            Mark::None
        }
    }

    pub fn is_banned(&self, id: &str) -> bool {
        self.banned.contains(id)
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.favorites.contains(id)
    }

    /// A place is a favorite, banned, or neither: setting one clears the other.
    pub fn set(&mut self, id: &str, mark: Mark) {
        self.favorites.remove(id);
        self.banned.remove(id);
        match mark {
            Mark::Favorite => self.favorites.insert(id.to_string()),
            Mark::Banned => self.banned.insert(id.to_string()),
            Mark::None => false,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_a_favorite_or_banned_never_both() {
        let mut m = Marks::default();
        m.set("n1", Mark::Favorite);
        assert_eq!(m.get("n1"), Mark::Favorite);
        m.set("n1", Mark::Banned);
        assert_eq!((m.get("n1"), m.is_favorite("n1")), (Mark::Banned, false));
        m.set("n1", Mark::None);
        assert_eq!(m.get("n1"), Mark::None);
        assert!(m.favorites.is_empty() && m.banned.is_empty());
    }

    #[test]
    fn unknown_places_are_unmarked_and_none_is_shared_and_empty() {
        assert_eq!(Marks::none().get("anything"), Mark::None);
    }
}
