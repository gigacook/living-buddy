-- Tendly initial schema (SQLite). Timestamps are RFC 3339 UTC strings.
-- Dates (YYYY-MM-DD) and times (HH:MM) for tasks are local to the row's timezone.

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE members (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    color TEXT NOT NULL,
    prefs TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- "groups" is an SQL keyword; the table is called tgroups.
CREATE TABLE tgroups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    mode TEXT NOT NULL,
    description TEXT,
    goal TEXT,
    start_date TEXT,
    end_date TEXT,
    columns TEXT NOT NULL DEFAULT '[]',
    archived INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE group_members (
    group_id TEXT NOT NULL REFERENCES tgroups(id) ON DELETE CASCADE,
    member_id TEXT NOT NULL REFERENCES members(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, member_id)
);

CREATE TABLE milestones (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES tgroups(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    due_date TEXT,
    done INTEGER NOT NULL DEFAULT 0,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    group_id TEXT REFERENCES tgroups(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    notes TEXT,
    category TEXT NOT NULL,
    priority TEXT NOT NULL,
    duration_minutes INTEGER,
    owner_id TEXT NOT NULL,
    assignee_id TEXT,
    due_date TEXT,
    due_time TEXT,
    start_date TEXT,
    start_time TEXT,
    deadline TEXT,
    timezone TEXT NOT NULL,
    recurrence TEXT,
    repeat_mode TEXT NOT NULL DEFAULT 'fixed',
    series_start TEXT,
    rotation TEXT NOT NULL DEFAULT '[]',
    column_key TEXT,
    milestone_id TEXT REFERENCES milestones(id) ON DELETE SET NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    subtasks TEXT NOT NULL DEFAULT '[]',
    position REAL NOT NULL DEFAULT 0,
    completed_at TEXT,
    completed_by TEXT,
    completion_count INTEGER NOT NULL DEFAULT 0,
    template_key TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    deleted_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX tasks_group ON tasks(group_id);
CREATE INDEX tasks_assignee ON tasks(assignee_id);
CREATE INDEX tasks_due ON tasks(due_date);

CREATE TABLE task_completions (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    occurrence_due TEXT,
    completed_by TEXT,
    completed_by_name TEXT NOT NULL,
    completed_at TEXT NOT NULL
);
CREATE INDEX completions_task ON task_completions(task_id, completed_at);

CREATE TABLE activity (
    id TEXT PRIMARY KEY,
    at TEXT NOT NULL,
    actor_id TEXT,
    actor_name TEXT NOT NULL,
    source TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    group_id TEXT,
    op TEXT NOT NULL,
    summary TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0,
    before_json TEXT,
    after_json TEXT
);
CREATE INDEX activity_entity ON activity(entity_type, entity_id, at);
CREATE INDEX activity_group ON activity(group_id, at);
CREATE INDEX activity_at ON activity(at);

CREATE TABLE routine_templates (
    key TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    category TEXT NOT NULL,
    recurrence TEXT NOT NULL,
    repeat_mode TEXT NOT NULL,
    duration_minutes INTEGER NOT NULL,
    checklist TEXT NOT NULL,
    tip TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE timers (
    member_id TEXT PRIMARY KEY REFERENCES members(id) ON DELETE CASCADE,
    state TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL
);

CREATE TABLE countdowns (
    id TEXT PRIMARY KEY,
    member_id TEXT NOT NULL REFERENCES members(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    date TEXT NOT NULL,
    time TEXT,
    timezone TEXT NOT NULL,
    target_at TEXT NOT NULL,
    category TEXT,
    group_id TEXT REFERENCES tgroups(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL
);

CREATE TABLE alarms (
    id TEXT PRIMARY KEY,
    member_id TEXT NOT NULL REFERENCES members(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    time TEXT NOT NULL,
    weekdays INTEGER NOT NULL DEFAULT 0,
    date TEXT,
    timezone TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    sound INTEGER NOT NULL DEFAULT 1,
    task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    last_fired_at TEXT,
    snoozed_until TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE usage_tracker (
    member_id TEXT PRIMARY KEY REFERENCES members(id) ON DELETE CASCADE,
    five_hour_percent REAL,
    five_hour_resets_at TEXT,
    seven_day_percent REAL,
    seven_day_resets_at TEXT,
    notes TEXT,
    reminder_thresholds TEXT NOT NULL DEFAULT '[]',
    remind_on_reset INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT
);

CREATE TABLE calendar_sources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    url_ciphertext TEXT,
    url_display TEXT,
    group_id TEXT REFERENCES tgroups(id) ON DELETE SET NULL,
    owner_id TEXT,
    is_private INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 100,
    enabled INTEGER NOT NULL DEFAULT 1,
    refresh_minutes INTEGER NOT NULL DEFAULT 60,
    etag TEXT,
    last_modified_header TEXT,
    last_fetched_at TEXT,
    last_status TEXT NOT NULL DEFAULT 'never',
    last_error TEXT,
    warnings TEXT NOT NULL DEFAULT '[]',
    default_tz TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE calendar_events (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES calendar_sources(id) ON DELETE CASCADE,
    uid TEXT NOT NULL,
    instance_key TEXT NOT NULL DEFAULT '',
    title TEXT NOT NULL,
    description TEXT,
    location TEXT,
    start_json TEXT NOT NULL,
    end_json TEXT NOT NULL,
    rrule TEXT,
    exdates TEXT NOT NULL DEFAULT '[]',
    rdates TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'confirmed',
    sequence INTEGER NOT NULL DEFAULT 0,
    dtstamp TEXT,
    last_modified TEXT,
    categories TEXT NOT NULL DEFAULT '[]',
    content_hash TEXT NOT NULL,
    category TEXT,
    group_id TEXT REFERENCES tgroups(id) ON DELETE SET NULL,
    local_override TEXT,
    removed_upstream INTEGER NOT NULL DEFAULT 0,
    revision INTEGER NOT NULL DEFAULT 1,
    start_utc TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (source_id, uid, instance_key)
);
CREATE INDEX events_source ON calendar_events(source_id);
CREATE INDEX events_uid ON calendar_events(uid);

CREATE TABLE share_links (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    label TEXT NOT NULL,
    scope TEXT NOT NULL,
    created_by TEXT,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT,
    last_used_at TEXT,
    use_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE notifications (
    id TEXT PRIMARY KEY,
    member_id TEXT NOT NULL REFERENCES members(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT,
    task_id TEXT,
    from_member_id TEXT,
    created_at TEXT NOT NULL,
    deliver_after TEXT NOT NULL,
    read_at TEXT
);
CREATE INDEX notifications_member ON notifications(member_id, deliver_after);

CREATE TABLE nudges (
    id TEXT PRIMARY KEY,
    from_member_id TEXT NOT NULL,
    to_member_id TEXT NOT NULL,
    task_id TEXT,
    at TEXT NOT NULL
);
CREATE INDEX nudges_pair ON nudges(from_member_id, to_member_id, at);

CREATE TABLE connectors (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    display_name TEXT NOT NULL,
    -- Suggestions from this connector are visible only to this person.
    owner_member_id TEXT REFERENCES members(id) ON DELETE SET NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'never_run',
    settings TEXT NOT NULL DEFAULT '{}',
    credentials_ciphertext TEXT,
    cursor TEXT,
    last_run_at TEXT,
    last_error TEXT,
    items_ingested INTEGER NOT NULL DEFAULT 0,
    ai_consent INTEGER NOT NULL DEFAULT 0,
    retention_days INTEGER NOT NULL DEFAULT 14,
    poll_minutes INTEGER NOT NULL DEFAULT 30,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE ingested_items (
    id TEXT PRIMARY KEY,
    connector_id TEXT REFERENCES connectors(id) ON DELETE CASCADE,
    external_hash TEXT NOT NULL,
    subject TEXT NOT NULL,
    excerpt TEXT,
    received_at TEXT NOT NULL,
    ingested_at TEXT NOT NULL,
    purge_after TEXT NOT NULL,
    UNIQUE (connector_id, external_hash)
);

CREATE TABLE suggestions (
    id TEXT PRIMARY KEY,
    item_id TEXT REFERENCES ingested_items(id) ON DELETE SET NULL,
    connector_id TEXT,
    connector_name TEXT NOT NULL,
    member_id TEXT REFERENCES members(id) ON DELETE CASCADE,
    subject TEXT NOT NULL,
    received_at TEXT NOT NULL,
    draft TEXT NOT NULL,
    extractor TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    decided_at TEXT,
    decided_by TEXT,
    result_type TEXT,
    result_id TEXT,
    purge_after TEXT NOT NULL
);
CREATE INDEX suggestions_status ON suggestions(status, created_at);

CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'queued',
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    run_after TEXT NOT NULL,
    locked_until TEXT,
    last_error TEXT,
    idempotency_key TEXT UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX jobs_ready ON jobs(status, run_after);

CREATE TABLE devices (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    last_seen_at TEXT,
    revoked_at TEXT
);

CREATE TABLE oauth_states (
    state_hash TEXT PRIMARY KEY,
    connector_id TEXT NOT NULL REFERENCES connectors(id) ON DELETE CASCADE,
    verifier_ciphertext TEXT NOT NULL,
    created_at TEXT NOT NULL
);
