//! Domain vocabulary: categories, priorities, group kinds and modes.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, TS)]
        #[ts(export)]
        pub enum $name {
            $($(#[$vmeta])* #[serde(rename = $text)] $variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(&self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($text => Some($name::$variant),)+ _ => None }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_enum!(
    /// Life areas. Each has a pastel identity in the UI, always paired with a label and icon.
    Category {
        Home => "home",
        Errands => "errands",
        People => "people",
        School => "school",
        Work => "work",
        Personal => "personal",
    }
);

string_enum!(Priority {
    Low => "low",
    Normal => "normal",
    High => "high",
});

string_enum!(
    /// Who a group is made of. Purely descriptive; it changes defaults and copy, not permissions.
    GroupKind {
        Solo => "solo",
        Partners => "partners",
        Family => "family",
        Friends => "friends",
        Roommates => "roommates",
        ProjectTeam => "project_team",
        Custom => "custom",
    }
);

string_enum!(
    /// Household mode is ongoing upkeep with no finish line; project mode has goals and dates.
    GroupMode {
        Household => "household",
        Project => "project",
    }
);

string_enum!(
    /// How a recurring task schedules its next occurrence.
    RepeatMode {
        /// Follow the calendar rule regardless of when it was done.
        Fixed => "fixed",
        /// Count the interval from the moment it was completed ("every 7 days after done").
        AfterCompletion => "after_completion",
    }
);

/// Default Kanban columns for project groups. Groups can rename, add and reorder columns.
pub const DEFAULT_BOARD_COLUMNS: &[(&str, &str)] =
    &[("backlog", "Backlog"), ("planned", "Planned"), ("in_progress", "In progress"), ("blocked", "Blocked"), ("done", "Done")];

/// The column key that marks a task as finished.
pub const DONE_COLUMN: &str = "done";

/// Validates a user-supplied display name. Names are attribution, not authentication.
pub fn normalize_display_name(raw: &str) -> Result<String, &'static str> {
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return Err("Please enter a name.");
    }
    if cleaned.chars().count() > 40 {
        return Err("Names can be up to 40 characters.");
    }
    Ok(cleaned)
}

/// Trims and bounds free text such as titles. Control characters (except newlines when allowed) are dropped.
pub fn clean_text(raw: &str, max_chars: usize, allow_newlines: bool) -> String {
    let filtered: String = raw.chars().filter(|c| !c.is_control() || (allow_newlines && *c == '\n')).collect();
    let trimmed = filtered.trim();
    trimmed.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_round_trip() {
        for c in Category::ALL {
            assert_eq!(Category::parse(c.as_str()), Some(*c));
        }
        assert_eq!(Category::parse("unknown"), None);
        assert_eq!(serde_json::to_string(&GroupKind::ProjectTeam).unwrap(), "\"project_team\"");
    }

    #[test]
    fn display_names_are_cleaned() {
        assert_eq!(normalize_display_name("  Ada \t Lovelace ").unwrap(), "Ada Lovelace");
        assert!(normalize_display_name("   ").is_err());
        assert!(normalize_display_name(&"x".repeat(41)).is_err());
        assert_eq!(normalize_display_name("Al\u{0007}ex").unwrap(), "Alex");
    }

    #[test]
    fn clean_text_bounds_and_strips() {
        assert_eq!(clean_text("  hi\u{0000} there ", 100, false), "hi there");
        assert_eq!(clean_text("a\nb", 100, true), "a\nb");
        assert_eq!(clean_text("a\nb", 100, false), "ab");
        assert_eq!(clean_text("abcdef", 3, false), "abc");
    }
}
