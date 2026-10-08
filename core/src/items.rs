//! Plain-language explanations of the items a quest can reward, for the activity log ("why did I get a Letter?").

/// What an item does, in one sentence. Items whose effect is not wired up yet say so (the log must not promise what the app does not do).
#[must_use]
pub fn blurb(item: &str) -> String {
    let text = match item {
        i if i.starts_with("Letter ") => "A letter for the Letter Hunt goal: collect them all to spell the word.",
        "Progressive Zone Key" => "A key: collect enough of them to open the next zone.",
        "Bike" | "Car" => "A travel tool: it unlocks the zone that needs it (ride a bike there).",
        "Progressive Effort Reduction" => "Meant to make quests easier (shorter effort). Not applied yet in this version.",
        "Progressive Scouting Distance" => "Lets the fog of war reveal quests from farther away.",
        "Progressive Collection Distance" => "Meant to widen how near you must get to collect. Not applied yet in this version.",
        "Hydrate!" => "Filler: drink some water, on your honor.",
        "Take a Breather!" => "Filler: take a short breather, on your honor.",
        "Fog Of War Trap" => "Trap: quests are hidden on the map for 15 minutes (you can still complete them).",
        "Freeze Trap" => "Trap: no quest counts until you reach the glowing thaw point.",
        "Silence Trap" => "Trap: notifications are muted for 15 minutes.",
        "Leash Trap" => "Trap: quests only count within 800 m of home for 30 minutes.",
        "Detour Trap" => "Trap: visit the marked waypoint before any quest counts.",
        "Toll Trap" => "Trap: cover 400 m before any quest counts.",
        "Slow Trap" => "Trap: dwell quests take twice as long for 30 minutes.",
        "Shuffle Trap" => "Trap: unfinished quests are rerolled to new places.",
        i if i.ends_with("Trap") => "Honor trap: do it on your honor, nothing is checked.",
        _ => "An item from the game; it has no effect in this version.",
    };
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_item_in_the_pool_is_explained() {
        for (item, needle) in [
            ("Letter P", "Letter Hunt"),
            ("Progressive Zone Key", "zone"),
            ("Bike", "bike"),
            ("Progressive Effort Reduction", "not applied"),
            ("Progressive Scouting Distance", "reveal"),
            ("Progressive Collection Distance", "not applied"),
            ("Hydrate!", "honor"),
            ("Take a Breather!", "honor"),
            ("Fog Of War Trap", "hidden"),
            ("Freeze Trap", "thaw"),
            ("Shuffle Trap", "rerolled"),
            ("Sit Up Trap", "honor"),
        ] {
            let b = blurb(item).to_lowercase();
            assert!(b.contains(&needle.to_lowercase()), "{item}: {b}");
        }
    }

    #[test]
    fn an_unknown_item_still_gets_a_sentence() {
        assert!(!blurb("Mystery Box").is_empty());
    }
}
