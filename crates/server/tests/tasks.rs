//! Tasks, recurring chores, rotation, reassignment history, Kanban and group scoping.

mod common;
use axum::http::StatusCode;
use chrono::Duration;
use common::{Req, TestApp};
use serde_json::json;

fn today(app: &TestApp) -> chrono::NaiveDate {
    app.state.now().date_naive()
}

#[tokio::test]
async fn task_crud_conflicts_and_history() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let b = app.member("Sam").await;
    let g = app.group(&a, "Home", "household", &[&a, &b]).await;
    let gid = g["id"].as_str().unwrap();
    let t = app
        .ok(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "  Water plants ", "groupId": gid, "category": "home", "subtasks": ["Kitchen", "Balcony"], "tags": ["Green"]})))
        .await;
    assert_eq!(t["title"], "Water plants");
    assert_eq!(t["tags"], json!(["green"]));
    assert_eq!(t["subtasks"].as_array().unwrap().len(), 2);
    let id = t["id"].as_str().unwrap();

    // Validation errors are field-level.
    let res = app.send(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "x", "dueDate": "2026-02-31"}))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.body["field"], "dueDate");

    // Reassign to Sam: activity + notification for Sam.
    let t2 = app.ok(Req::new("PATCH", format!("/api/tasks/{id}")).actor(&a).json(json!({"expectedVersion": 1, "changes": {"assigneeId": b}}))).await;
    assert_eq!(t2["assigneeId"], b.as_str());
    assert_eq!(t2["version"], 2);
    let notes = app.ok(Req::new("GET", "/api/notifications").actor(&b)).await;
    assert!(notes.as_array().unwrap().iter().any(|n| n["kind"] == "assignment"));

    // A stale edit conflicts and returns the current version.
    let res = app.send(Req::new("PATCH", format!("/api/tasks/{id}")).actor(&b).json(json!({"expectedVersion": 1, "changes": {"title": "Mine"}}))).await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    assert_eq!(res.body["current"]["version"], 2);

    // Assignee must be in the group.
    let c = app.member("Outsider").await;
    let res = app.send(Req::new("PATCH", format!("/api/tasks/{id}")).actor(&a).json(json!({"expectedVersion": 2, "changes": {"assigneeId": c}}))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    let h = app.ok(Req::new("GET", format!("/api/tasks/{id}/history")).actor(&a)).await;
    let ops: Vec<&str> = h["activity"].as_array().unwrap().iter().map(|x| x["op"].as_str().unwrap()).collect();
    assert_eq!(ops, vec!["reassign", "create"]);
    let reassign = &h["activity"][0];
    assert_eq!(reassign["actorName"], "Alex");
    assert!(reassign["summary"].as_str().unwrap().contains("to Sam"));
    assert_eq!(reassign["before"]["assigneeId"], serde_json::Value::Null);
    assert_eq!(reassign["after"]["assigneeId"], b.as_str());

    // Delete is soft and recorded.
    app.ok(Req::new("DELETE", format!("/api/tasks/{id}")).actor(&a)).await;
    assert_eq!(app.send(Req::new("GET", format!("/api/tasks/{id}")).actor(&a)).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn recurring_chore_rotates_and_keeps_history() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let b = app.member("Sam").await;
    let g = app.group(&a, "Home", "household", &[&a, &b]).await;
    let gid = g["id"].as_str().unwrap();
    let due = today(&app).format("%Y-%m-%d").to_string();
    let t = app
        .ok(Req::new("POST", "/api/templates/dishes/use").actor(&a).json(json!({"groupId": gid, "rotation": [a, b], "dueDate": due, "timezone": "UTC"})))
        .await;
    assert_eq!(t["recurrence"], "FREQ=DAILY");
    assert_eq!(t["assigneeId"], a.as_str());
    assert_eq!(t["templateKey"], "dishes");
    let id = t["id"].as_str().unwrap().to_string();

    // Tick one checklist item, then complete.
    let mut subs = t["subtasks"].clone();
    subs[0]["done"] = json!(true);
    app.ok(Req::new("PATCH", format!("/api/tasks/{id}")).actor(&a).json(json!({"expectedVersion": 1, "changes": {"subtasks": subs}}))).await;
    let r = app.ok(Req::new("POST", format!("/api/tasks/{id}/complete")).actor(&a)).await;
    let tomorrow = (today(&app) + Duration::days(1)).format("%Y-%m-%d").to_string();
    assert_eq!(r["nextDueDate"], tomorrow.as_str());
    assert_eq!(r["task"]["assigneeId"], b.as_str(), "rotation hands it to Sam");
    assert_eq!(r["task"]["completedAt"], serde_json::Value::Null, "recurring tasks stay open");
    assert_eq!(r["task"]["completionCount"], 1);
    assert!(r["task"]["subtasks"].as_array().unwrap().iter().all(|s| s["done"] == false), "checklist resets");

    // Sam completes the next one; it rotates back to Alex.
    let r2 = app.ok(Req::new("POST", format!("/api/tasks/{id}/complete")).actor(&b)).await;
    assert_eq!(r2["task"]["assigneeId"], a.as_str());
    let h = app.ok(Req::new("GET", format!("/api/tasks/{id}/history")).actor(&a)).await;
    let comps = h["completions"].as_array().unwrap();
    assert_eq!(comps.len(), 2);
    assert_eq!(comps[0]["completedByName"], "Sam");
    assert_eq!(comps[1]["completedByName"], "Alex");
    assert_eq!(comps[1]["occurrenceDue"], due.as_str());
}

#[tokio::test]
async fn overdue_fixed_chore_skips_missed_occurrences_and_after_completion_counts_from_done() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let long_ago = (today(&app) - Duration::days(20)).format("%Y-%m-%d").to_string();
    let t = app
        .ok(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "Water plants", "dueDate": long_ago, "recurrence": "FREQ=WEEKLY", "timezone": "UTC"})))
        .await;
    let r = app.ok(Req::new("POST", format!("/api/tasks/{}/complete", t["id"].as_str().unwrap())).actor(&a)).await;
    let next = chrono::NaiveDate::parse_from_str(r["nextDueDate"].as_str().unwrap(), "%Y-%m-%d").unwrap();
    assert!(next > today(&app), "no pile of missed occurrences");
    assert!(next <= today(&app) + Duration::days(7));
    assert_eq!((next - (today(&app) - Duration::days(20))).num_days() % 7, 0, "stays on the weekly schedule");

    let t = app
        .ok(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "Clean filter", "dueDate": long_ago, "recurrence": "FREQ=DAILY;INTERVAL=10", "repeatMode": "after_completion", "timezone": "UTC"})))
        .await;
    assert!(t["recurrenceLabel"].as_str().unwrap().contains("after done"));
    let r = app.ok(Req::new("POST", format!("/api/tasks/{}/complete", t["id"].as_str().unwrap())).actor(&a)).await;
    assert_eq!(r["nextDueDate"], (today(&app) + Duration::days(10)).format("%Y-%m-%d").to_string().as_str());
}

#[tokio::test]
async fn kanban_transitions() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let g = app.group(&a, "Garden", "project", &[&a]).await;
    let gid = g["id"].as_str().unwrap();
    let cols: Vec<&str> = g["columns"].as_array().unwrap().iter().map(|c| c["key"].as_str().unwrap()).collect();
    assert_eq!(cols, vec!["backlog", "planned", "in_progress", "blocked", "done"]);
    let t = app.ok(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "Buy soil", "groupId": gid}))).await;
    assert_eq!(t["columnKey"], "backlog");
    let id = t["id"].as_str().unwrap();
    let t = app.ok(Req::new("POST", format!("/api/tasks/{id}/move")).actor(&a).json(json!({"expectedVersion": 1, "columnKey": "in_progress"}))).await;
    assert_eq!(t["columnKey"], "in_progress");
    assert_eq!(t["completedAt"], serde_json::Value::Null);
    let res = app.send(Req::new("POST", format!("/api/tasks/{id}/move")).actor(&a).json(json!({"expectedVersion": 2, "columnKey": "nowhere"}))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    let res = app.send(Req::new("POST", format!("/api/tasks/{id}/move")).actor(&a).json(json!({"expectedVersion": 1, "columnKey": "done"}))).await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    let t = app.ok(Req::new("POST", format!("/api/tasks/{id}/move")).actor(&a).json(json!({"expectedVersion": 2, "columnKey": "done"}))).await;
    assert!(t["completedAt"].is_string(), "done column completes the task");
    let t = app.ok(Req::new("POST", format!("/api/tasks/{id}/move")).actor(&a).json(json!({"expectedVersion": 3, "columnKey": "blocked"}))).await;
    assert_eq!(t["completedAt"], serde_json::Value::Null, "leaving done reopens");
    // Custom columns: rename and remove one; tasks in removed columns move to the first column.
    let g2 = app
        .ok(Req::new("PATCH", format!("/api/groups/{gid}")).actor(&a).json(json!({"columns": [{"key": "backlog", "name": "Ideas"}, {"name": "Doing"}, {"key": "done", "name": "Done"}]})))
        .await;
    assert_eq!(g2["columns"][1]["key"], "doing");
    let t = app.ok(Req::new("GET", format!("/api/tasks/{id}")).actor(&a)).await;
    assert_eq!(t["columnKey"], "backlog");
    let res = app.send(Req::new("PATCH", format!("/api/groups/{gid}")).actor(&a).json(json!({"columns": [{"name": "Only"}]}))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST, "done column is required");
    let h = app.ok(Req::new("GET", format!("/api/tasks/{id}/history")).actor(&a)).await;
    assert!(h["activity"].as_array().unwrap().iter().filter(|x| x["op"] == "move").count() >= 3);
}

#[tokio::test]
async fn group_scoping_and_personal_privacy() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let b = app.member("Sam").await;
    let g = app.group(&a, "Alex only", "household", &[&a]).await;
    let gid = g["id"].as_str().unwrap();
    let gt = app.ok(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "Group secret", "groupId": gid}))).await;
    let pt = app.ok(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "Personal thing"}))).await;
    // Sam sees neither in the default list.
    let list = app.ok(Req::new("GET", "/api/tasks").actor(&b)).await;
    assert!(list.as_array().unwrap().is_empty());
    assert_eq!(app.send(Req::new("GET", format!("/api/tasks?groupId={gid}")).actor(&b)).await.status, StatusCode::FORBIDDEN);
    for id in [gt["id"].as_str().unwrap(), pt["id"].as_str().unwrap()] {
        assert_eq!(app.send(Req::new("GET", format!("/api/tasks/{id}")).actor(&b)).await.status, StatusCode::FORBIDDEN);
        assert_eq!(app.send(Req::new("POST", format!("/api/tasks/{id}/complete")).actor(&b)).await.status, StatusCode::FORBIDDEN);
    }
    assert_eq!(app.send(Req::new("POST", "/api/tasks").actor(&b).json(json!({"title": "x", "groupId": gid}))).await.status, StatusCode::FORBIDDEN);
    // Personal tasks can't be assigned to others.
    let res = app.send(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "x", "assigneeId": b}))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    // Requests without a chosen name are rejected.
    assert_eq!(app.send(Req::new("GET", "/api/tasks")).await.status, StatusCode::BAD_REQUEST);
    // After joining, Sam sees the group task but still not the personal one.
    app.ok(Req::new("POST", format!("/api/groups/{gid}/join")).actor(&b)).await;
    let list = app.ok(Req::new("GET", "/api/tasks").actor(&b)).await;
    let titles: Vec<&str> = list.as_array().unwrap().iter().map(|t| t["title"].as_str().unwrap()).collect();
    assert_eq!(titles, vec!["Group secret"]);
    // Activity feed is scoped the same way.
    let c = app.member("Robin").await;
    let feed = app.ok(Req::new("GET", "/api/activity").actor(&c)).await;
    assert!(!feed.to_string().contains("Group secret") && !feed.to_string().contains("Personal thing"));
}

#[tokio::test]
async fn nudges_are_opt_in_and_rate_limited() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let b = app.member("Sam").await;
    app.group(&a, "Home", "household", &[&a, &b]).await;
    let res = app.send(Req::new("POST", "/api/nudges").actor(&a).json(json!({"toMemberId": b}))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST, "Sam has not opted in");
    let sam = app.ok(Req::new("GET", "/api/members").actor(&b)).await;
    let mut prefs = sam.as_array().unwrap().iter().find(|m| m["id"] == b.as_str()).unwrap()["prefs"].clone();
    prefs["acceptsNudges"] = json!(true);
    prefs["quietStart"] = json!(null);
    prefs["quietEnd"] = json!(null);
    // Only Sam can change Sam's preferences.
    assert_eq!(app.send(Req::new("PATCH", format!("/api/members/{b}")).actor(&a).json(json!({"prefs": prefs}))).await.status, StatusCode::FORBIDDEN);
    app.ok(Req::new("PATCH", format!("/api/members/{b}")).actor(&b).json(json!({"prefs": prefs}))).await;
    let t = app.ok(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "Laundry"}))).await;
    let tid = t["id"].as_str().unwrap();
    app.ok(Req::new("POST", "/api/nudges").actor(&a).json(json!({"toMemberId": b, "taskId": tid, "message": "when you can 🙂"}))).await;
    let res = app.send(Req::new("POST", "/api/nudges").actor(&a).json(json!({"toMemberId": b, "taskId": tid}))).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST, "per-task cooldown");
    let n = app.ok(Req::new("GET", "/api/notifications").actor(&b)).await;
    assert!(n.as_array().unwrap().iter().any(|x| x["kind"] == "nudge"));
    let outsider = app.member("Robin").await;
    let res = app.send(Req::new("POST", "/api/nudges").actor(&outsider).json(json!({"toMemberId": b}))).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN, "no shared group");
}

#[tokio::test]
async fn timer_survives_sleep_and_pomodoro_waits() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let v = app
        .ok(Req::new("POST", "/api/timer").actor(&a).json(json!({"command": {"action": "start", "kind": "focus", "minutes": 25, "label": "Deep work", "taskId": null, "config": null}})))
        .await;
    assert_eq!(v["state"]["status"], "running");
    let near = |v: &serde_json::Value, ms: i64| (v["remainingMs"].as_i64().unwrap() - ms).abs() < 2_000;
    assert!(near(&v, 25 * 60_000));
    // Simulate the laptop sleeping for 10 minutes: the deadline is authoritative.
    app.state.clock.advance(Duration::minutes(10));
    let v = app.ok(Req::new("GET", "/api/timer").actor(&a)).await;
    assert!(near(&v, 15 * 60_000), "{v}");
    app.state.clock.advance(Duration::hours(3));
    let v = app.ok(Req::new("GET", "/api/timer").actor(&a)).await;
    assert_eq!(v["state"]["status"], "finished");
    assert_eq!(v["phaseJustEnded"], true);
    let v = app.ok(Req::new("GET", "/api/timer").actor(&a)).await;
    assert_eq!(v["phaseJustEnded"], false, "ending is reported once");

    let v = app.ok(Req::new("POST", "/api/timer").actor(&a).json(json!({"command": {"action": "start", "kind": "pomodoro", "minutes": null, "label": null, "taskId": null, "config": null}}))).await;
    let version = v["version"].as_u64().unwrap();
    let res = app.send(Req::new("POST", "/api/timer").actor(&a).json(json!({"command": {"action": "pause"}, "expectedVersion": version - 1}))).await;
    assert_eq!(res.status, StatusCode::CONFLICT, "another device changed it");
    app.state.clock.advance(Duration::hours(2));
    let v = app.ok(Req::new("GET", "/api/timer").actor(&a)).await;
    assert_eq!(v["state"]["status"], "phase_complete");
    assert_eq!(v["state"]["phase"], "short_break");
    let v = app.ok(Req::new("POST", "/api/timer").actor(&a).json(json!({"command": {"action": "skip"}}))).await;
    assert_eq!(v["state"]["phase"], "focus");
    assert_eq!(v["state"]["status"], "running");
}

#[tokio::test]
async fn alarms_countdowns_and_usage() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let al = app.ok(Req::new("POST", "/api/alarms").actor(&a).json(json!({"label": "Meds", "time": "08:00", "weekdays": 0, "timezone": "UTC"}))).await;
    assert!(al["nextFireAt"].is_string());
    let id = al["id"].as_str().unwrap();
    app.state.clock.advance(Duration::days(2));
    let fired = app.ok(Req::new("POST", format!("/api/alarms/{id}/fired")).actor(&a)).await;
    let next = chrono::DateTime::parse_from_rfc3339(fired["nextFireAt"].as_str().unwrap()).unwrap();
    assert!(next > app.state.now(), "a missed alarm fires once, then moves on");
    let sn = app.ok(Req::new("POST", format!("/api/alarms/{id}/snooze")).actor(&a).json(json!({"minutes": 10}))).await;
    assert_eq!(sn["nextFireAt"], sn["snoozedUntil"]);
    assert_eq!(app.send(Req::new("POST", "/api/alarms").actor(&a).json(json!({"label": "x", "time": "8am"}))).await.status, StatusCode::BAD_REQUEST);

    app.ok(Req::new("POST", "/api/countdowns").actor(&a).json(json!({"title": "Exam", "date": "2030-06-01", "timezone": "Europe/Stockholm"}))).await;
    let cds = app.ok(Req::new("GET", "/api/countdowns").actor(&a)).await;
    assert_eq!(cds[0]["targetAt"], "2030-05-31T22:00:00Z");

    let u = app.ok(Req::new("GET", "/api/usage").actor(&a)).await;
    assert_eq!(u["fiveHour"]["state"], "unknown");
    assert_eq!(u["source"], "manual");
    let resets = (app.state.now() + Duration::hours(2)).to_rfc3339();
    let u = app
        .ok(Req::new("PUT", "/api/usage").actor(&a).json(json!({"fiveHourPercent": 60, "fiveHourResetsAt": resets, "sevenDayPercent": 55, "reminderThresholds": [50, 75], "remindOnReset": true, "notes": "estimate from settings page"})))
        .await;
    assert_eq!(u["fiveHour"]["reached"], json!([25, 50]));
    assert_eq!(u["sevenDay"]["reached"], json!([50]));
    let n = app.ok(Req::new("GET", "/api/notifications").actor(&a)).await;
    assert!(n.as_array().unwrap().iter().any(|x| x["kind"] == "usage"));
    assert_eq!(app.send(Req::new("PUT", "/api/usage").actor(&a).json(json!({"fiveHourPercent": 150}))).await.status, StatusCode::BAD_REQUEST);
    app.state.clock.advance(Duration::hours(3));
    let u = app.ok(Req::new("GET", "/api/usage").actor(&a)).await;
    assert_eq!(u["fiveHour"]["state"], "reset_passed");
}
