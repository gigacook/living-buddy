//! Deterministic rules for refreshing feeds and merging calendars.
//!
//! * Events are keyed by `(source, UID, RECURRENCE-ID)`; refreshing a feed
//!   updates rows in place instead of inserting duplicates.
//! * A higher SEQUENCE always wins; on equal SEQUENCE a newer LAST-MODIFIED
//!   wins; identical content is a no-op.
//! * Events that disappear from a feed are marked cancelled for *that source
//!   only*; nothing from other sources or local data is ever touched.
//! * In merged views the same UID from several sources is shown once. The
//!   winner is chosen by SEQUENCE, then LAST-MODIFIED, then source priority,
//!   then source id, so the result never depends on fetch order.

use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredVersion {
    pub sequence: i64,
    pub last_modified: Option<DateTime<Utc>>,
    pub content_hash: String,
    pub cancelled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpsertDecision {
    Insert,
    Update { reason: &'static str },
    Skip { reason: &'static str },
}

pub fn decide_upsert(existing: Option<&StoredVersion>, incoming: &StoredVersion) -> UpsertDecision {
    let Some(old) = existing else { return UpsertDecision::Insert };
    if incoming.sequence < old.sequence {
        return UpsertDecision::Skip { reason: "older sequence" };
    }
    if incoming.sequence > old.sequence {
        return UpsertDecision::Update { reason: "newer sequence" };
    }
    if incoming.content_hash == old.content_hash && incoming.cancelled == old.cancelled {
        return UpsertDecision::Skip { reason: "unchanged" };
    }
    match (incoming.last_modified, old.last_modified) {
        (Some(new), Some(prev)) if new < prev => UpsertDecision::Skip { reason: "older last-modified" },
        _ => UpsertDecision::Update { reason: "content changed" },
    }
}

/// Keys present in storage for a source but missing from the refreshed feed.
/// Only keys of the *same* source are ever passed in, so other sources are safe.
pub fn missing_from_feed(stored: &BTreeSet<String>, incoming: &BTreeSet<String>) -> Vec<String> {
    stored.difference(incoming).cloned().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeCandidate {
    pub id: String,
    pub source_id: String,
    pub source_priority: i64,
    pub uid: String,
    pub instance_key: String,
    pub sequence: i64,
    pub last_modified: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergedPick {
    pub winner_id: String,
    /// Source ids that also carried this event (excluding the winner's source).
    pub also_in: Vec<String>,
}

/// Collapses duplicates across sources. Events from local (non-feed) data use
/// unique UIDs and therefore never collapse with feed events by accident.
pub fn dedupe(candidates: &[MergeCandidate]) -> Vec<MergedPick> {
    let mut groups: BTreeMap<(String, String), Vec<&MergeCandidate>> = BTreeMap::new();
    for c in candidates {
        groups.entry((c.uid.clone(), c.instance_key.clone())).or_default().push(c);
    }
    let mut out = Vec::new();
    for (_, mut group) in groups {
        group.sort_by(|a, b| {
            b.sequence
                .cmp(&a.sequence)
                .then(b.last_modified.cmp(&a.last_modified))
                .then(a.source_priority.cmp(&b.source_priority))
                .then(a.source_id.cmp(&b.source_id))
                .then(a.id.cmp(&b.id))
        });
        let winner = group[0];
        let mut also: Vec<String> = group[1..]
            .iter()
            .map(|c| c.source_id.clone())
            .filter(|s| *s != winner.source_id)
            .collect();
        also.sort();
        also.dedup();
        out.push(MergedPick { winner_id: winner.id.clone(), also_in: also });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn v(seq: i64, lm: Option<i64>, hash: &str) -> StoredVersion {
        StoredVersion {
            sequence: seq,
            last_modified: lm.map(|s| Utc.timestamp_opt(s, 0).unwrap()),
            content_hash: hash.into(),
            cancelled: false,
        }
    }

    #[test]
    fn upsert_rules() {
        assert_eq!(decide_upsert(None, &v(0, None, "a")), UpsertDecision::Insert);
        assert!(matches!(decide_upsert(Some(&v(2, None, "a")), &v(1, None, "b")), UpsertDecision::Skip { .. }));
        assert!(matches!(decide_upsert(Some(&v(1, None, "a")), &v(2, None, "a")), UpsertDecision::Update { .. }));
        assert_eq!(decide_upsert(Some(&v(1, Some(10), "a")), &v(1, Some(10), "a")), UpsertDecision::Skip { reason: "unchanged" });
        assert!(matches!(decide_upsert(Some(&v(1, Some(20), "a")), &v(1, Some(10), "b")), UpsertDecision::Skip { .. }));
        assert!(matches!(decide_upsert(Some(&v(1, Some(10), "a")), &v(1, Some(20), "b")), UpsertDecision::Update { .. }));
        assert!(matches!(decide_upsert(Some(&v(1, None, "a")), &v(1, None, "b")), UpsertDecision::Update { .. }));
    }

    #[test]
    fn missing_only_within_source() {
        let stored: BTreeSet<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let incoming: BTreeSet<String> = ["a", "c", "d"].iter().map(|s| s.to_string()).collect();
        assert_eq!(missing_from_feed(&stored, &incoming), vec!["b".to_string()]);
    }

    fn c(id: &str, src: &str, prio: i64, uid: &str, seq: i64, lm: Option<i64>) -> MergeCandidate {
        MergeCandidate {
            id: id.into(),
            source_id: src.into(),
            source_priority: prio,
            uid: uid.into(),
            instance_key: "20261010T100000Z".into(),
            sequence: seq,
            last_modified: lm.map(|s| Utc.timestamp_opt(s, 0).unwrap()),
        }
    }

    #[test]
    fn dedupe_is_deterministic() {
        let a = c("e1", "s1", 1, "u", 1, Some(5));
        let b = c("e2", "s2", 0, "u", 1, Some(5));
        let d = c("e3", "s3", 0, "other", 0, None);
        let picks1 = dedupe(&[a.clone(), b.clone(), d.clone()]);
        let picks2 = dedupe(&[d, b, a]);
        assert_eq!(picks1, picks2);
        let u = picks1.iter().find(|p| p.winner_id == "e2").unwrap();
        assert_eq!(u.also_in, vec!["s1".to_string()]);
        assert_eq!(picks1.len(), 2);
    }

    #[test]
    fn dedupe_prefers_higher_sequence_then_newer() {
        let a = c("e1", "s1", 0, "u", 3, Some(1));
        let b = c("e2", "s2", 0, "u", 2, Some(99));
        assert_eq!(dedupe(&[a.clone(), b.clone()])[0].winner_id, "e1");
        let a = c("e1", "s1", 0, "u", 2, Some(1));
        assert_eq!(dedupe(&[a, b])[0].winner_id, "e2");
    }
}
