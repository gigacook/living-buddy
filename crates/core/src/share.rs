//! Read-only share links are bearer capabilities. A scope must name what it
//! exposes explicitly; nothing private is included by default.

use crate::model::Category;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ShareDetail {
    /// Titles, times, locations and notes.
    Full,
    /// Titles and times only.
    #[default]
    TitlesOnly,
    /// Only "Busy" blocks.
    BusyOnly,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ShareScope {
    /// Groups whose events and tasks may appear.
    pub group_ids: Vec<String>,
    /// Restrict to items assigned to / owned by these people (empty = anyone in scope).
    pub member_ids: Vec<String>,
    /// Restrict to these categories (empty = all).
    pub categories: Vec<Category>,
    /// Calendar sources to include. Private sources appear only when listed here.
    pub source_ids: Vec<String>,
    pub include_tasks: bool,
    /// Personal (non-group) tasks of `member_ids`. Off unless explicitly enabled.
    pub include_personal: bool,
    pub detail: ShareDetail,
    pub days_back: u32,
    pub days_ahead: u32,
}

impl ShareScope {
    pub fn validate(&self) -> Result<(), &'static str> {
        let selects_something =
            !self.group_ids.is_empty() || !self.source_ids.is_empty() || (self.include_personal && !self.member_ids.is_empty());
        if !selects_something {
            return Err("Choose at least one group, calendar or person to share.");
        }
        if self.include_personal && self.member_ids.is_empty() {
            return Err("Personal tasks can only be shared for specific people.");
        }
        if self.days_back > 90 || self.days_ahead > 400 || self.days_ahead == 0 {
            return Err("Share between 1 and 400 days ahead and up to 90 days back.");
        }
        if self.group_ids.len() + self.member_ids.len() + self.source_ids.len() > 200 {
            return Err("That scope is too large.");
        }
        Ok(())
    }

    pub fn category_ok(&self, c: Option<Category>) -> bool {
        self.categories.is_empty() || c.map(|c| self.categories.contains(&c)).unwrap_or(false)
    }
}

pub struct TaskFacts<'a> {
    pub group_id: Option<&'a str>,
    pub owner_id: &'a str,
    pub assignee_id: Option<&'a str>,
    pub category: Option<Category>,
}

pub fn allows_task(scope: &ShareScope, t: &TaskFacts) -> bool {
    if !scope.include_tasks || !scope.category_ok(t.category) {
        return false;
    }
    match t.group_id {
        Some(g) => {
            scope.group_ids.iter().any(|x| x == g)
                && (scope.member_ids.is_empty() || t.assignee_id.map(|a| scope.member_ids.iter().any(|m| m == a)).unwrap_or(false))
        }
        None => scope.include_personal && scope.member_ids.iter().any(|m| m == t.owner_id),
    }
}

pub struct EventFacts<'a> {
    pub source_id: &'a str,
    pub source_private: bool,
    pub group_id: Option<&'a str>,
    pub category: Option<Category>,
}

pub fn allows_event(scope: &ShareScope, e: &EventFacts) -> bool {
    if !scope.category_ok(e.category) {
        return false;
    }
    let listed = scope.source_ids.iter().any(|s| s == e.source_id);
    if e.source_private && !listed {
        return false;
    }
    if listed {
        return true;
    }
    // Not explicitly listed: allowed only through a shared group in scope.
    match e.group_id {
        Some(g) => scope.group_ids.iter().any(|x| x == g),
        None => false,
    }
}

/// Only the hash of a share token is stored; the token itself is shown once.
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> ShareScope {
        ShareScope { group_ids: vec!["g1".into()], include_tasks: true, days_ahead: 30, ..Default::default() }
    }

    #[test]
    fn validation() {
        assert!(ShareScope { days_ahead: 30, ..Default::default() }.validate().is_err());
        assert!(scope().validate().is_ok());
        let personal_no_member = ShareScope { include_personal: true, ..scope() };
        assert!(personal_no_member.validate().is_err());
    }

    #[test]
    fn personal_tasks_never_leak_by_default() {
        let s = scope();
        let personal = TaskFacts { group_id: None, owner_id: "m1", assignee_id: Some("m1"), category: None };
        assert!(!allows_task(&s, &personal));
        let other_group = TaskFacts { group_id: Some("g2"), owner_id: "m1", assignee_id: None, category: None };
        assert!(!allows_task(&s, &other_group));
        let ok = TaskFacts { group_id: Some("g1"), owner_id: "m1", assignee_id: None, category: Some(Category::Home) };
        assert!(allows_task(&s, &ok));
        let s2 = ShareScope { include_personal: true, member_ids: vec!["m1".into()], ..scope() };
        assert!(allows_task(&s2, &personal));
        let s3 = ShareScope { categories: vec![Category::Work], ..scope() };
        assert!(!allows_task(&s3, &ok));
    }

    #[test]
    fn private_sources_need_explicit_listing() {
        let s = scope();
        let private = EventFacts { source_id: "src", source_private: true, group_id: Some("g1"), category: None };
        assert!(!allows_event(&s, &private));
        let s2 = ShareScope { source_ids: vec!["src".into()], ..scope() };
        assert!(allows_event(&s2, &private));
        let shared_group = EventFacts { source_id: "x", source_private: false, group_id: Some("g1"), category: None };
        assert!(allows_event(&s, &shared_group));
        let ungrouped = EventFacts { source_id: "x", source_private: false, group_id: None, category: None };
        assert!(!allows_event(&s, &ungrouped));
    }

    #[test]
    fn token_hash_is_stable() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), hash_token("abd"));
        assert_eq!(hash_token("abc").len(), 64);
    }
}
