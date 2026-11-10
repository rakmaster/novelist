//! Novel types and the background-note categories suggested for each.

pub struct Genre {
    pub name: &'static str,
    /// (category, prompt shown in empty entries)
    pub categories: &'static [(&'static str, &'static str)],
}

pub const DEFAULT_GENRE: &str = "General fiction";

pub const GENRES: &[Genre] = &[
    Genre {
        name: DEFAULT_GENRE,
        categories: &[
            ("Locations", "Places that appear in the story: what they look and feel like, who is found there."),
            ("Timeline", "Dated events, in order, including backstory and what happens off the page."),
            ("Themes & Motifs", "Ideas, images and symbols the book keeps returning to."),
            ("Research", "Facts, sources and details to get right."),
            ("Style Notes", "Voice, tense, point of view, spelling and naming conventions."),
        ],
    },
    Genre {
        name: "Fantasy",
        categories: &[
            ("Magic System", "Where magic comes from, what it can and cannot do, its cost, and who can use it."),
            ("Setting & Era", "Technology level, how long the world has been this way, daily life."),
            ("Nations", "Each nation: geography, government, economy, military, character."),
            ("Society", "Classes, laws, customs, morality, how ordinary people live."),
            ("Guilds & Organizations", "Factions, their purpose, ranks, rivalries and secrets."),
            ("Religions", "Gods, beliefs, practices and who actually believes them."),
            ("Calendar & Dates", "Months, seasons, how years are counted."),
            ("History", "Past events that shaped the present."),
            ("Creatures", "Animals, monsters and other peoples."),
            ("Items & Technology", "Notable tools, weapons and devices, and the rules around them."),
        ],
    },
    Genre {
        name: "Science fiction",
        categories: &[
            ("Technology", "Key inventions, how they work, who controls them, their side effects."),
            ("Science & Rules", "The scientific assumptions of the world and where it bends reality."),
            ("Setting & Era", "When and where this takes place, and what changed from today."),
            ("Planets & Locations", "Worlds, stations, ships: environment, population, politics."),
            ("Factions & Governments", "Powers, alliances, conflicts and their motives."),
            ("Species", "Non-human or modified peoples: biology, culture, language."),
            ("History & Timeline", "The events that led here, with dates."),
            ("Calendar & Dates", "How time is measured."),
        ],
    },
    Genre {
        name: "Mystery / thriller",
        categories: &[
            ("Locations", "Crime scenes and key places: layout, access, who was there."),
            ("Timeline of Events", "What happened, minute by minute where it matters, including what the culprit did."),
            ("Clues & Evidence", "Each clue: where it is found, what it seems to mean, what it really means."),
            ("Suspects & Alibis", "Motive, means, opportunity and alibi for each suspect."),
            ("Red Herrings", "Misleading leads and how they are resolved."),
            ("Procedure & Jargon", "Police, legal, forensic or professional detail to get right."),
        ],
    },
    Genre {
        name: "Historical",
        categories: &[
            ("Period & Setting", "Date range, region, the political and social situation."),
            ("Timeline", "Real events alongside the story's events."),
            ("Real People & Events", "Historical figures and incidents used, and how far the book departs from record."),
            ("Locations", "Places as they were then, with sources."),
            ("Customs & Daily Life", "Food, clothing, work, manners, money, travel."),
            ("Language & Slang", "Period-appropriate words and forms of address; things to avoid."),
            ("Research", "Sources, questions still open, facts to verify."),
        ],
    },
    Genre {
        name: "Romance",
        categories: &[
            ("Locations", "Where the story happens and what each place means to the couple."),
            ("Relationship Timeline", "Key beats: meeting, attraction, conflict, low point, resolution."),
            ("Tropes & Themes", "Tropes you are using or subverting, and the emotional theme."),
            ("Family & Friends", "Supporting cast and how they push or pull the couple."),
            ("Research", "Professions, places and details to get right."),
        ],
    },
    Genre {
        name: "Literary / contemporary",
        categories: &[
            ("Locations", "Settings and the mood each carries."),
            ("Timeline", "Chronology, including flashbacks and gaps."),
            ("Themes & Motifs", "What the book is about beneath the plot; recurring images."),
            ("Research", "Facts, sources and details to get right."),
            ("Style Notes", "Voice, structure, tense and point of view."),
        ],
    },
];

pub fn genre(name: &str) -> &'static Genre {
    GENRES.iter().find(|g| g.name == name).unwrap_or(&GENRES[0])
}

/// Prompt text for a category in any genre (case-insensitive).
pub fn hint_for(category: &str) -> Option<&'static str> {
    GENRES
        .iter()
        .flat_map(|g| g.categories.iter())
        .find(|(c, _)| c.eq_ignore_ascii_case(category.trim()))
        .map(|(_, h)| *h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Book;

    #[test]
    fn unknown_genre_falls_back() {
        assert_eq!(genre("nope").name, DEFAULT_GENRE);
        assert!(hint_for("magic system").is_some());
        assert!(hint_for("Nonexistent").is_none());
    }

    #[test]
    fn starter_entries_skip_existing_categories() {
        let mut b = Book::default();
        b.genre = "Fantasy".into();
        b.add_world_entry("Nations", "Great Andrews");
        let added = b.add_starter_entries();
        assert_eq!(added, genre("Fantasy").categories.len() - 1);
        assert_eq!(b.add_starter_entries(), 0);
        assert!(b.world.iter().any(|e| e.category == "Magic System" && e.name == "Magic System"));
    }
}
