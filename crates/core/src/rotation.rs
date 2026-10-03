//! Simple responsibility rotation for shared chores.

/// Who is responsible next. Rotation advances from the person who was
/// responsible for the occurrence that just finished, so helping out on
/// someone else's turn does not reshuffle the order.
pub fn next_assignee(rotation: &[String], current: Option<&str>) -> Option<String> {
    if rotation.is_empty() {
        return None;
    }
    let idx = current.and_then(|c| rotation.iter().position(|m| m == c));
    let next = match idx {
        Some(i) => (i + 1) % rotation.len(),
        None => 0,
    };
    Some(rotation[next].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_and_wraps() {
        let r: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(next_assignee(&r, Some("a")).as_deref(), Some("b"));
        assert_eq!(next_assignee(&r, Some("c")).as_deref(), Some("a"));
        assert_eq!(next_assignee(&r, Some("zz")).as_deref(), Some("a"));
        assert_eq!(next_assignee(&r, None).as_deref(), Some("a"));
        assert_eq!(next_assignee(&[], Some("a")), None);
    }
}
