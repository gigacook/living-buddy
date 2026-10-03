//! Calendar import/refresh merging, filtering, exports, change history and share links.

mod common;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use common::{Req, TestApp};
use serde_json::{json, Value};

fn ics(events: &[String]) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nX-WR-CALNAME:School\r\nX-WR-TIMEZONE:Europe/Stockholm\r\n{}END:VCALENDAR\r\n",
        events.join("")
    )
}

fn vevent(uid: &str, summary: &str, start: &str, seq: i64, extra: &str) -> String {
    format!("BEGIN:VEVENT\r\nUID:{uid}\r\nDTSTAMP:20261001T000000Z\r\nDTSTART;TZID=Europe/Stockholm:{start}\r\nDURATION:PT1H\r\nSUMMARY:{summary}\r\nSEQUENCE:{seq}\r\n{extra}END:VEVENT\r\n")
}

fn base_date() -> chrono::NaiveDate {
    Utc::now().date_naive() + Duration::days(3)
}

fn stamp(d: chrono::NaiveDate, hm: &str) -> String {
    format!("{}T{hm}00", d.format("%Y%m%d"))
}

async fn occurrences(app: &TestApp, actor: &str, query: &str) -> Vec<Value> {
    let from = (Utc::now() - Duration::days(1)).format("%Y-%m-%d");
    let to = (Utc::now() + Duration::days(40)).format("%Y-%m-%d");
    app.ok(Req::new("GET", format!("/api/calendar/occurrences?from={from}&to={to}&includeTasks=false{query}")).actor(actor))
        .await
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn import_reimport_merges_without_duplicates() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let d = base_date();
    let v1 = ics(&[
        vevent("lesson@example.test", "Piano lesson", &stamp(d, "1600"), 0, "RRULE:FREQ=WEEKLY;COUNT=4\r\nCATEGORIES:School\r\n"),
        vevent("trip@example.test", "Field trip", &stamp(d, "0900"), 0, ""),
        vevent("gone@example.test", "Parent meeting", &stamp(d, "1900"), 0, ""),
    ]);
    let r = app.ok(Req::new("POST", "/api/calendar/import?name=School").actor(&a).raw(v1.clone())).await;
    assert_eq!(r["inserted"], 3);
    let sid = r["source"]["id"].as_str().unwrap().to_string();
    assert_eq!(occurrences(&app, &a, "").await.len(), 6);

    // Re-importing the same file changes nothing.
    let r = app.ok(Req::new("POST", format!("/api/calendar/import?sourceId={sid}")).actor(&a).raw(v1)).await;
    assert_eq!((r["inserted"].as_u64(), r["updated"].as_u64(), r["unchanged"].as_u64()), (Some(0), Some(0), Some(3)));

    // A local override on the trip survives the next refresh.
    let occ = occurrences(&app, &a, "").await;
    let trip = occ.iter().find(|o| o["title"] == "Field trip").unwrap();
    let ev = app.ok(Req::new("GET", format!("/api/calendar/events/{}", trip["eventId"].as_str().unwrap())).actor(&a)).await;
    assert_eq!(ev["editable"], false);
    let rev = ev["revision"].as_u64().unwrap();
    // Feed events cannot be edited directly (no provider write-back)...
    let res = app
        .send(
            Req::new("PATCH", format!("/api/calendar/events/{}", ev["id"].as_str().unwrap())).actor(&a).json(
                json!({"expectedRevision": rev, "event": {"sourceId": sid, "title": "x", "allDay": true, "startDate": "2026-01-01"}}),
            ),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    // ...but accept local overrides.
    app.ok(Req::new("PATCH", format!("/api/calendar/events/{}", ev["id"].as_str().unwrap()))
        .actor(&a)
        .json(json!({"expectedRevision": rev, "localOverride": {"category": "school", "note": "Bring lunch"}})))
        .await;

    // v2: lesson moved (sequence 1), meeting removed upstream, trip unchanged.
    let v2 = ics(&[
        vevent(
            "lesson@example.test",
            "Piano lesson (new room)",
            &stamp(d, "1700"),
            1,
            "RRULE:FREQ=WEEKLY;COUNT=4\r\nCATEGORIES:School\r\n",
        ),
        vevent("trip@example.test", "Field trip", &stamp(d, "0900"), 0, ""),
    ]);
    let r = app.ok(Req::new("POST", format!("/api/calendar/import?sourceId={sid}")).actor(&a).raw(v2)).await;
    assert_eq!((r["inserted"].as_u64(), r["updated"].as_u64(), r["cancelled"].as_u64()), (Some(0), Some(1), Some(1)));
    let occ = occurrences(&app, &a, "").await;
    assert_eq!(occ.len(), 5);
    assert!(occ.iter().all(|o| o["title"] != "Parent meeting"));
    let lesson = occ.iter().find(|o| o["title"] == "Piano lesson (new room)").unwrap();
    assert!(lesson["start"].as_str().unwrap().contains("T15:00:00") || lesson["start"].as_str().unwrap().contains("T16:00:00"));
    let trip = occ.iter().find(|o| o["title"] == "Field trip").unwrap();
    assert_eq!(trip["category"], "school");
    assert!(trip["description"].as_str().unwrap().contains("Bring lunch"));

    // An older sequence never overwrites a newer one.
    let stale = ics(&[
        vevent("lesson@example.test", "Piano lesson (old)", &stamp(d, "1600"), 0, "RRULE:FREQ=WEEKLY;COUNT=4\r\n"),
        vevent("trip@example.test", "Field trip", &stamp(d, "0900"), 0, ""),
    ]);
    app.ok(Req::new("POST", format!("/api/calendar/import?sourceId={sid}")).actor(&a).raw(stale)).await;
    assert!(occurrences(&app, &a, "").await.iter().any(|o| o["title"] == "Piano lesson (new room)"));

    // Change history attributes file imports to the person and records operations and revisions.
    let changes = app.ok(Req::new("GET", format!("/api/calendar/changes?sourceId={sid}")).actor(&a)).await;
    let ops: Vec<&str> = changes.as_array().unwrap().iter().map(|c| c["op"].as_str().unwrap()).collect();
    for op in ["create", "update", "removed_upstream", "import", "override"] {
        assert!(ops.contains(&op), "missing {op} in {ops:?}");
    }
    let upd = changes.as_array().unwrap().iter().find(|c| c["op"] == "update").unwrap();
    assert_eq!(upd["actorName"], "Alex");
    assert!(upd["revision"].as_u64().unwrap() >= 2);
    assert!(upd["before"]["title"].is_string() && upd["after"]["title"].is_string());
}

#[tokio::test]
async fn merged_sources_dedupe_and_filter_and_never_touch_other_sources() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let g = app.group(&a, "Home", "household", &[&a]).await;
    let gid = g["id"].as_str().unwrap();
    let d = base_date();
    let shared = vevent("shared@example.test", "Concert", &stamp(d, "2000"), 2, "");
    let r1 = app
        .ok(Req::new("POST", format!("/api/calendar/import?name=Mine&groupId={gid}"))
            .actor(&a)
            .raw(ics(&[shared.clone(), vevent("only1@example.test", "Gym", &stamp(d, "0700"), 0, "CATEGORIES:Personal\r\n")])))
        .await;
    let r2 = app
        .ok(Req::new("POST", "/api/calendar/import?name=Theirs").actor(&a).raw(ics(&[vevent(
            "shared@example.test",
            "Concert",
            &stamp(d, "2000"),
            1,
            "",
        )])))
        .await;
    let s1 = r1["source"]["id"].as_str().unwrap();
    let s2 = r2["source"]["id"].as_str().unwrap();
    let occ = occurrences(&app, &a, "").await;
    let concerts: Vec<&Value> = occ.iter().filter(|o| o["title"] == "Concert").collect();
    assert_eq!(concerts.len(), 1, "same UID from two sources is shown once");
    assert_eq!(concerts[0]["sourceId"], s1, "higher SEQUENCE wins deterministically");
    assert_eq!(concerts[0]["alsoIn"], json!([s2]));

    assert_eq!(occurrences(&app, &a, "&category=personal").await.len(), 1);
    assert_eq!(occurrences(&app, &a, &format!("&groupId={gid}")).await.len(), 2);
    assert_eq!(occurrences(&app, &a, &format!("&sourceId={s2}")).await.len(), 1);

    // Re-importing source 2 with nothing in it removes only source 2's events.
    let r = app.ok(Req::new("POST", format!("/api/calendar/import?sourceId={s2}")).actor(&a).raw(ics(&[]))).await;
    assert_eq!(r["cancelled"], 1);
    let occ = occurrences(&app, &a, "").await;
    assert_eq!(occ.len(), 2, "source 1 is untouched");
    // Deleting a source removes only its own events.
    app.ok(Req::new("DELETE", format!("/api/calendar/sources/{s2}")).actor(&a)).await;
    assert_eq!(occurrences(&app, &a, "").await.len(), 2);
}

#[tokio::test]
async fn ics_export_round_trip_with_all_day_recurrence_and_dst() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let src = app.ok(Req::new("POST", "/api/calendar/sources").actor(&a).json(json!({"name": "Mine", "kind": "local"}))).await;
    let sid = src["id"].as_str().unwrap();
    let start = base_date().format("%Y-%m-%d").to_string();
    app.ok(Req::new("POST", "/api/calendar/events").actor(&a).json(json!({
        "sourceId": sid, "title": "Yoga; with \"friends\", maybe", "description": "Line one\nLine two", "allDay": false,
        "startDate": start, "startTime": "08:30", "endTime": "09:30", "timezone": "America/New_York", "recurrence": "FREQ=WEEKLY;COUNT=8", "category": "personal"
    }))).await;
    app.ok(Req::new("POST", "/api/calendar/events").actor(&a).json(json!({"sourceId": sid, "title": "Holiday", "allDay": true, "startDate": start, "endDate": (base_date() + Duration::days(2)).format("%Y-%m-%d").to_string()}))).await;
    let task =
        app.ok(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "Pay rent", "dueDate": start, "timezone": "UTC"}))).await;

    let from = Utc::now().format("%Y-%m-%d");
    let to = (Utc::now() + Duration::days(120)).format("%Y-%m-%d");
    let res = app.send(Req::new("GET", format!("/api/calendar/export.ics?from={from}&to={to}")).actor(&a)).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.headers.get("content-type").unwrap().to_str().unwrap().starts_with("text/calendar"));
    let text = res.text;
    assert!(text.contains("BEGIN:VTIMEZONE\r\nTZID:America/New_York"));
    assert!(text.contains("RRULE:FREQ=WEEKLY;COUNT=8"));
    assert!(text.contains("SUMMARY:Yoga\\; with \"friends\"\\, maybe"));
    assert!(text.contains("DESCRIPTION:Line one\\nLine two"));
    assert!(text.contains(&format!("UID:task-{}@tendly", task["id"].as_str().unwrap())));
    let parsed = tendly_core::ics::parse(&text, &tendly_core::ics::Limits::default()).unwrap();
    assert_eq!(parsed.events.len(), 3);
    let yoga = parsed.events.iter().find(|e| e.summary.starts_with("Yoga")).unwrap();
    assert_eq!(yoga.description.as_deref(), Some("Line one\nLine two"));
    assert!(yoga.dtstamp.is_some() && yoga.last_modified.is_some());
    // 8 weekly occurrences that cross the November DST change keep 08:30 local time.
    let (occ, _) = tendly_core::ics::expand_occurrences(
        &yoga.start,
        &yoga.effective_end(),
        yoga.rrule.as_deref(),
        &yoga.exdates,
        &[],
        chrono_tz::Tz::UTC,
        Utc::now() - Duration::days(1),
        Utc::now() + Duration::days(120),
        100,
    );
    assert_eq!(occ.len(), 8);
    for o in &occ {
        assert_eq!(o.start.local().format("%H:%M").to_string(), "08:30");
    }
    let holiday = parsed.events.iter().find(|e| e.summary == "Holiday").unwrap();
    assert!(holiday.start.is_date());
    // DTEND is exclusive: a 3-day holiday ends the day after its last day.
    if let (tendly_core::ics::IcsTime::Date { date: s }, Some(tendly_core::ics::IcsTime::Date { date: e })) = (&holiday.start, &holiday.end)
    {
        assert_eq!((*e - *s).num_days(), 3);
    } else {
        panic!("holiday should be all-day");
    }
    // Re-importing our own export creates no duplicates of its events.
    let r = app.ok(Req::new("POST", "/api/calendar/import?name=Roundtrip").actor(&a).raw(text.clone())).await;
    assert_eq!(r["inserted"], 3);
    let r = app
        .ok(Req::new("POST", format!("/api/calendar/import?sourceId={}", r["source"]["id"].as_str().unwrap())).actor(&a).raw(text))
        .await;
    assert_eq!(r["unchanged"], 3);

    let j = app.ok(Req::new("GET", format!("/api/calendar/export.json?from={from}&to={to}")).actor(&a)).await;
    assert!(j["items"].as_array().unwrap().len() >= 10);
}

#[tokio::test]
async fn hostile_calendar_input_is_rejected_or_neutralized() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let res = app.send(Req::new("POST", "/api/calendar/import").actor(&a).raw("<html>not a calendar</html>")).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    let big = "X".repeat(7 * 1024 * 1024);
    let res = app.send(Req::new("POST", "/api/calendar/import").actor(&a).raw(big)).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
    // Subscribing to internal addresses is refused (SSRF protection).
    for url in
        ["http://169.254.169.254/latest/meta-data/", "http://127.0.0.1:7878/api/admin/export", "file:///etc/passwd", "http://[::1]/x.ics"]
    {
        let res = app.send(Req::new("POST", "/api/calendar/sources").actor(&a).json(json!({"name": "x", "kind": "url", "url": url}))).await;
        if res.status == StatusCode::OK {
            assert_eq!(res.body["lastStatus"], "error", "{url}");
            assert!(res.body["lastError"].as_str().unwrap().contains("private or internal"), "{url}");
            assert!(!res.body["urlDisplay"].as_str().unwrap().contains("meta-data"));
        } else {
            assert_eq!(res.status, StatusCode::BAD_REQUEST, "{url}");
        }
    }
    // Script in event text stays inert in share HTML.
    app.enable_sharing(&a).await;
    let src = app
        .ok(Req::new("POST", "/api/calendar/import?name=Evil").actor(&a).raw(ics(&[vevent(
            "x@test",
            "<script>alert(1)</script>",
            &stamp(base_date(), "1000"),
            0,
            "DESCRIPTION:<img src=x onerror=alert(1)>\r\n",
        )])))
        .await;
    let link = app
        .ok(Req::new("POST", "/api/shares")
            .actor(&a)
            .json(json!({"label": "x", "scope": {"sourceIds": [src["source"]["id"]], "detail": "full", "daysAhead": 30}})))
        .await;
    let html = app.send(Req::new("GET", link["htmlPath"].as_str().unwrap()).no_csrf()).await;
    assert!(!html.text.contains("<script>alert"));
    assert!(html.text.contains("&lt;script&gt;"));
    assert!(!html.text.contains("<img src=x"));
}

#[tokio::test]
async fn share_links_are_scoped_revocable_and_off_by_default() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let b = app.member("Sam").await;
    let g = app.group(&a, "Home", "household", &[&a, &b]).await;
    let gid = g["id"].as_str().unwrap();
    let start = base_date().format("%Y-%m-%d").to_string();
    app.ok(Req::new("POST", "/api/tasks")
        .actor(&a)
        .json(json!({"title": "Shared chore", "groupId": gid, "dueDate": start, "notes": "door code 1234"})))
        .await;
    app.ok(Req::new("POST", "/api/tasks").actor(&a).json(json!({"title": "Private therapy", "dueDate": start}))).await;
    let private_src = app
        .ok(Req::new("POST", "/api/calendar/import?name=Private").actor(&a).raw(ics(&[vevent(
            "p@test",
            "Private appointment",
            &stamp(base_date(), "1000"),
            0,
            "",
        )])))
        .await;
    let group_src = app
        .ok(Req::new("POST", format!("/api/calendar/import?name=Family&groupId={gid}&isPrivate=false")).actor(&a).raw(ics(&[vevent(
            "f@test",
            "Family dinner",
            &stamp(base_date(), "1800"),
            0,
            "LOCATION:Grandma's\r\n",
        )])))
        .await;
    let _ = private_src;
    let _ = group_src;

    let scope = json!({"groupIds": [gid], "includeTasks": true, "detail": "titles_only", "daysAhead": 30});
    let res = app.send(Req::new("POST", "/api/shares").actor(&a).json(json!({"label": "Family", "scope": scope}))).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN, "sharing is off by default");
    app.enable_sharing(&a).await;
    let outsider = app.member("Robin").await;
    let res = app.send(Req::new("POST", "/api/shares").actor(&outsider).json(json!({"label": "x", "scope": scope}))).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN, "must be in the group to share it");
    let res = app
        .send(
            Req::new("POST", "/api/shares")
                .actor(&a)
                .json(json!({"label": "x", "scope": {"includePersonal": true, "memberIds": [b], "daysAhead": 30}})),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN, "cannot share someone else's personal tasks");

    let link = app.ok(Req::new("POST", "/api/shares").actor(&a).json(json!({"label": "Family", "scope": scope}))).await;
    let token = link["token"].as_str().unwrap();
    assert!(token.len() >= 43);
    let stored: String = sqlx::query_scalar("SELECT token_hash FROM share_links").fetch_one(&app.state.db).await.unwrap();
    assert_ne!(stored, token, "only the hash is stored");

    let j = app.send(Req::new("GET", link["jsonPath"].as_str().unwrap()).no_csrf()).await;
    assert_eq!(j.status, StatusCode::OK);
    let titles: Vec<&str> = j.body["items"].as_array().unwrap().iter().map(|i| i["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Shared chore") && titles.contains(&"Family dinner"));
    assert!(!titles.contains(&"Private therapy") && !titles.contains(&"Private appointment"));
    assert!(!j.text.contains("door code") && !j.text.contains("Grandma"), "titles-only hides notes and locations");
    let html = app.send(Req::new("GET", link["htmlPath"].as_str().unwrap()).no_csrf()).await;
    assert_eq!(html.headers.get("x-robots-tag").unwrap(), "noindex, nofollow");
    assert_eq!(html.headers.get("referrer-policy").unwrap(), "no-referrer");
    assert_eq!(html.headers.get("cache-control").unwrap(), "private, no-store");
    let ics_res = app.send(Req::new("GET", link["icsPath"].as_str().unwrap()).no_csrf()).await;
    assert!(ics_res.text.contains("SUMMARY:Family dinner") && !ics_res.text.contains("Private"));

    // Busy-only links reveal nothing but time blocks.
    let busy = app
        .ok(Req::new("POST", "/api/shares")
            .actor(&a)
            .json(json!({"label": "Busy", "scope": {"groupIds": [gid], "detail": "busy_only", "daysAhead": 30}})))
        .await;
    let bj = app.send(Req::new("GET", busy["jsonPath"].as_str().unwrap()).no_csrf()).await;
    assert!(bj.body["items"].as_array().unwrap().iter().all(|i| i["title"] == "Busy"));
    assert!(!bj.text.contains("f@test"));

    // Revocation is immediate; unknown, revoked and malformed tokens look the same.
    app.ok(Req::new("POST", format!("/api/shares/{}/revoke", link["link"]["id"].as_str().unwrap())).actor(&b)).await;
    let gone = app.send(Req::new("GET", link["jsonPath"].as_str().unwrap()).no_csrf()).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    let unknown = app.send(Req::new("GET", format!("/share/{}", "A".repeat(43))).no_csrf()).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(gone.text, unknown.text);
    // Turning sharing off stops every link.
    app.ok(Req::new("PATCH", "/api/admin/settings").actor(&a).json(json!({"sharingEnabled": false}))).await;
    assert_eq!(app.send(Req::new("GET", busy["jsonPath"].as_str().unwrap()).no_csrf()).await.status, StatusCode::NOT_FOUND);
    let changes = app.ok(Req::new("GET", "/api/calendar/changes").actor(&a)).await;
    let ops: Vec<&str> = changes.as_array().unwrap().iter().map(|c| c["op"].as_str().unwrap()).collect();
    assert!(ops.contains(&"publish"));
}

#[tokio::test]
async fn private_sources_are_invisible_to_others() {
    let app = TestApp::new().await;
    let a = app.member("Alex").await;
    let b = app.member("Sam").await;
    let r = app
        .ok(Req::new("POST", "/api/calendar/import?name=Mine").actor(&a).raw(ics(&[vevent(
            "m@test",
            "Therapy",
            &stamp(base_date(), "1000"),
            0,
            "",
        )])))
        .await;
    let sid = r["source"]["id"].as_str().unwrap();
    assert!(occurrences(&app, &b, "").await.is_empty());
    let srcs = app.ok(Req::new("GET", "/api/calendar/sources").actor(&b)).await;
    assert!(srcs.as_array().unwrap().is_empty());
    assert_eq!(app.send(Req::new("DELETE", format!("/api/calendar/sources/{sid}")).actor(&b)).await.status, StatusCode::FORBIDDEN);
    let ics = app.send(Req::new("GET", "/api/calendar/export.ics").actor(&b)).await;
    assert!(!ics.text.contains("Therapy"));
}
