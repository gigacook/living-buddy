//! HTTP/Tauri API contracts. TypeScript types are generated from these with
//! `cargo test` (see `packages/contracts`).

use crate::extraction::SuggestionDraft;
use crate::model::{Category, GroupKind, GroupMode, Priority, RepeatMode};
use crate::share::ShareScope;
use crate::timer::TimerState;
use crate::usage::WindowSummary;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ServerMode {
    /// Loopback only. The default.
    Local,
    /// Explicitly exposed to an allowlisted local network. Names are attribution, not authentication.
    Lan,
    /// Reachable remotely; every device must be paired and authenticated.
    Remote,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionInfo {
    pub mode: ServerMode,
    pub version: String,
    pub admin_available: bool,
    pub sharing_enabled: bool,
    pub device_auth_required: bool,
    pub device_paired: bool,
    pub ai_configured: bool,
    pub default_timezone: String,
    pub server_time: DateTime<Utc>,
    pub demo_data: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// For conflicts: the current server version of the object.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<Value>,
}

// ---------------------------------------------------------------- members

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemberPrefs {
    pub accepts_nudges: bool,
    pub quiet_start: Option<String>,
    pub quiet_end: Option<String>,
    pub timezone: String,
    pub show_usage_card: bool,
}

impl Default for MemberPrefs {
    fn default() -> Self {
        MemberPrefs {
            accepts_nudges: false,
            quiet_start: Some("21:30".into()),
            quiet_end: Some("07:30".into()),
            timezone: "UTC".into(),
            show_usage_card: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Member {
    pub id: String,
    pub display_name: String,
    pub color: String,
    pub prefs: MemberPrefs,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemberInput {
    pub display_name: String,
    #[ts(optional)]
    pub timezone: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct MemberPatch {
    #[ts(optional)]
    pub display_name: Option<String>,
    #[ts(optional)]
    pub color: Option<String>,
    #[ts(optional)]
    pub prefs: Option<MemberPrefs>,
}

// ---------------------------------------------------------------- groups

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub kind: GroupKind,
    pub mode: GroupMode,
    pub description: Option<String>,
    pub goal: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub member_ids: Vec<String>,
    pub columns: Vec<BoardColumn>,
    pub milestones: Vec<Milestone>,
    pub created_at: DateTime<Utc>,
    pub archived: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GroupInput {
    pub name: String,
    pub kind: GroupKind,
    pub mode: GroupMode,
    #[ts(optional)]
    pub description: Option<String>,
    #[ts(optional)]
    pub goal: Option<String>,
    #[ts(optional)]
    pub start_date: Option<String>,
    #[ts(optional)]
    pub end_date: Option<String>,
    #[ts(optional)]
    pub member_ids: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct GroupPatch {
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub kind: Option<GroupKind>,
    #[ts(optional)]
    pub description: Option<String>,
    #[ts(optional)]
    pub goal: Option<String>,
    #[ts(optional)]
    pub start_date: Option<String>,
    #[ts(optional)]
    pub end_date: Option<String>,
    #[ts(optional)]
    pub member_ids: Option<Vec<String>>,
    #[ts(optional)]
    pub archived: Option<bool>,
    /// Replaces the board columns (keys are kept stable; `done` must remain).
    #[ts(optional)]
    pub columns: Option<Vec<BoardColumnInput>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardColumn {
    pub key: String,
    pub name: String,
    pub position: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardColumnInput {
    #[ts(optional)]
    pub key: Option<String>,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Milestone {
    pub id: String,
    pub title: String,
    pub due_date: Option<String>,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MilestoneInput {
    pub title: String,
    #[ts(optional)]
    pub due_date: Option<String>,
    #[ts(optional)]
    pub done: Option<bool>,
}

// ---------------------------------------------------------------- tasks

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Subtask {
    pub id: String,
    pub title: String,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Task {
    pub id: String,
    pub group_id: Option<String>,
    pub title: String,
    pub notes: Option<String>,
    pub category: Category,
    pub priority: Priority,
    pub duration_minutes: Option<u32>,
    pub owner_id: String,
    pub assignee_id: Option<String>,
    /// YYYY-MM-DD in `timezone`.
    pub due_date: Option<String>,
    /// HH:MM in `timezone`.
    pub due_time: Option<String>,
    pub start_date: Option<String>,
    pub start_time: Option<String>,
    /// A hard deadline, distinct from the soft due date.
    pub deadline: Option<String>,
    pub timezone: String,
    pub recurrence: Option<String>,
    pub recurrence_label: Option<String>,
    pub repeat_mode: RepeatMode,
    pub rotation: Vec<String>,
    pub column_key: Option<String>,
    pub milestone_id: Option<String>,
    pub tags: Vec<String>,
    pub subtasks: Vec<Subtask>,
    pub position: f64,
    pub completed_at: Option<DateTime<Utc>>,
    pub completed_by: Option<String>,
    pub completion_count: u32,
    pub template_key: Option<String>,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct TaskInput {
    pub title: String,
    #[ts(optional)]
    pub group_id: Option<String>,
    #[ts(optional)]
    pub notes: Option<String>,
    #[ts(optional)]
    pub category: Option<Category>,
    #[ts(optional)]
    pub priority: Option<Priority>,
    #[ts(optional)]
    pub duration_minutes: Option<u32>,
    #[ts(optional)]
    pub assignee_id: Option<String>,
    #[ts(optional)]
    pub due_date: Option<String>,
    #[ts(optional)]
    pub due_time: Option<String>,
    #[ts(optional)]
    pub start_date: Option<String>,
    #[ts(optional)]
    pub start_time: Option<String>,
    #[ts(optional)]
    pub deadline: Option<String>,
    #[ts(optional)]
    pub timezone: Option<String>,
    #[ts(optional)]
    pub recurrence: Option<String>,
    #[ts(optional)]
    pub repeat_mode: Option<RepeatMode>,
    #[ts(optional)]
    pub rotation: Option<Vec<String>>,
    #[ts(optional)]
    pub column_key: Option<String>,
    #[ts(optional)]
    pub milestone_id: Option<String>,
    #[ts(optional)]
    pub tags: Option<Vec<String>>,
    #[ts(optional)]
    pub subtasks: Option<Vec<String>>,
    #[ts(optional)]
    pub template_key: Option<String>,
}

/// Partial update. `null` clears a field; a missing key leaves it unchanged.
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskPatch {
    pub expected_version: u32,
    #[ts(type = "Record<string, unknown>")]
    pub changes: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MoveTaskInput {
    pub expected_version: u32,
    pub column_key: String,
    #[ts(optional)]
    pub position: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompleteTaskResult {
    pub task: Task,
    /// For recurring tasks: the date the next occurrence is due.
    pub next_due_date: Option<String>,
    pub next_assignee_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskCompletion {
    pub id: String,
    pub task_id: String,
    pub occurrence_due: Option<String>,
    pub completed_by: Option<String>,
    pub completed_by_name: String,
    pub completed_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Activity {
    pub id: String,
    pub at: DateTime<Utc>,
    pub actor_id: Option<String>,
    /// Declared display name at the time, or "external source / unknown actor".
    pub actor_name: String,
    pub source: String,
    pub entity_type: String,
    pub entity_id: String,
    pub group_id: Option<String>,
    pub op: String,
    pub summary: String,
    pub revision: u32,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoutineTemplate {
    pub key: String,
    pub title: String,
    pub category: Category,
    pub recurrence: String,
    pub recurrence_label: String,
    pub repeat_mode: RepeatMode,
    pub duration_minutes: u32,
    pub checklist: Vec<String>,
    pub tip: String,
}

// ---------------------------------------------------------------- focus

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TimerView {
    pub state: TimerState,
    #[ts(type = "number")]
    pub remaining_ms: i64,
    pub server_now: DateTime<Utc>,
    pub version: u32,
    /// True when this request observed a phase ending (useful for notifications).
    pub phase_just_ended: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Countdown {
    pub id: String,
    pub title: String,
    pub date: String,
    pub time: Option<String>,
    pub timezone: String,
    pub target_at: DateTime<Utc>,
    pub category: Option<Category>,
    pub group_id: Option<String>,
    pub member_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CountdownInput {
    pub title: String,
    pub date: String,
    #[ts(optional)]
    pub time: Option<String>,
    #[ts(optional)]
    pub timezone: Option<String>,
    #[ts(optional)]
    pub category: Option<Category>,
    #[ts(optional)]
    pub group_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Alarm {
    pub id: String,
    pub member_id: String,
    pub label: String,
    pub time: String,
    /// Bit 0 = Monday ... bit 6 = Sunday; 0 = every day.
    pub weekdays: u8,
    pub date: Option<String>,
    pub timezone: String,
    pub enabled: bool,
    pub sound: bool,
    pub task_id: Option<String>,
    pub next_fire_at: Option<DateTime<Utc>>,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub snoozed_until: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlarmInput {
    pub label: String,
    pub time: String,
    #[ts(optional)]
    pub weekdays: Option<u8>,
    #[ts(optional)]
    pub date: Option<String>,
    #[ts(optional)]
    pub timezone: Option<String>,
    #[ts(optional)]
    pub enabled: Option<bool>,
    #[ts(optional)]
    pub sound: Option<bool>,
    #[ts(optional)]
    pub task_id: Option<String>,
}

// ---------------------------------------------------------------- usage tracker

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UsageView {
    pub five_hour: WindowSummary,
    pub seven_day: WindowSummary,
    pub notes: Option<String>,
    /// Always "manual" unless an officially supported adapter is configured.
    pub source: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub reminder_thresholds: Vec<u8>,
    pub remind_on_reset: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct UsageInput {
    pub five_hour_percent: Option<f32>,
    pub five_hour_resets_at: Option<DateTime<Utc>>,
    pub seven_day_percent: Option<f32>,
    pub seven_day_resets_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub reminder_thresholds: Vec<u8>,
    pub remind_on_reset: bool,
}

// ---------------------------------------------------------------- calendar

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarSource {
    pub id: String,
    pub name: String,
    /// "local", "file" or "url".
    pub kind: String,
    /// Scheme and host only; the full URL may contain secrets and is never returned.
    pub url_display: Option<String>,
    pub group_id: Option<String>,
    pub owner_id: Option<String>,
    pub is_private: bool,
    pub priority: i32,
    pub enabled: bool,
    pub refresh_minutes: u32,
    pub last_fetched_at: Option<DateTime<Utc>>,
    pub last_status: String,
    pub last_error: Option<String>,
    pub event_count: u32,
    pub warnings: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarSourceInput {
    pub name: String,
    /// "local" or "url". File imports use the import endpoint.
    pub kind: String,
    #[ts(optional)]
    pub url: Option<String>,
    #[ts(optional)]
    pub group_id: Option<String>,
    #[ts(optional)]
    pub is_private: Option<bool>,
    #[ts(optional)]
    pub refresh_minutes: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct CalendarSourcePatch {
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub group_id: Option<Option<String>>,
    #[ts(optional)]
    pub is_private: Option<bool>,
    #[ts(optional)]
    pub enabled: Option<bool>,
    #[ts(optional)]
    pub priority: Option<i32>,
    #[ts(optional)]
    pub refresh_minutes: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportResult {
    pub source: CalendarSource,
    pub inserted: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub cancelled: u32,
    pub warnings: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EventOccurrence {
    /// "event" for calendar events, "task" for tasks with due dates.
    pub kind: String,
    pub event_id: String,
    pub source_id: Option<String>,
    pub source_name: Option<String>,
    pub instance_key: String,
    pub uid: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    /// For all-day items: first day (inclusive).
    pub start_date: Option<String>,
    /// For all-day items: last day (inclusive).
    pub end_date: Option<String>,
    pub status: String,
    pub recurring: bool,
    pub category: Option<Category>,
    pub group_id: Option<String>,
    pub assignee_id: Option<String>,
    pub also_in: Vec<String>,
    pub has_local_override: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarEvent {
    pub id: String,
    pub source_id: String,
    pub uid: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub all_day: bool,
    pub start_date: String,
    pub start_time: Option<String>,
    pub end_date: String,
    pub end_time: Option<String>,
    pub timezone: String,
    pub recurrence: Option<String>,
    pub status: String,
    pub sequence: i32,
    pub category: Option<Category>,
    pub group_id: Option<String>,
    pub revision: u32,
    pub local_override: Option<LocalOverride>,
    pub editable: bool,
    pub updated_at: DateTime<Utc>,
}

/// Local-only adjustments to events from external feeds. They survive refreshes.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct LocalOverride {
    #[ts(optional)]
    pub category: Option<Category>,
    #[ts(optional)]
    pub hidden: Option<bool>,
    #[ts(optional)]
    pub note: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarEventInput {
    pub source_id: String,
    pub title: String,
    #[ts(optional)]
    pub description: Option<String>,
    #[ts(optional)]
    pub location: Option<String>,
    pub all_day: bool,
    pub start_date: String,
    #[ts(optional)]
    pub start_time: Option<String>,
    #[ts(optional)]
    pub end_date: Option<String>,
    #[ts(optional)]
    pub end_time: Option<String>,
    #[ts(optional)]
    pub timezone: Option<String>,
    #[ts(optional)]
    pub recurrence: Option<String>,
    #[ts(optional)]
    pub category: Option<Category>,
    #[ts(optional)]
    pub group_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarEventPatch {
    pub expected_revision: u32,
    #[ts(optional)]
    pub event: Option<CalendarEventInput>,
    #[ts(optional)]
    pub local_override: Option<LocalOverride>,
    #[ts(optional)]
    pub cancel: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShareLink {
    pub id: String,
    pub label: String,
    pub scope: ShareScope,
    pub created_at: DateTime<Utc>,
    pub created_by: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub use_count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShareLinkInput {
    pub label: String,
    pub scope: ShareScope,
    #[ts(optional)]
    pub expires_in_days: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreatedShareLink {
    pub link: ShareLink,
    /// Shown once. Only a hash is stored on the server.
    pub token: String,
    pub html_path: String,
    pub ics_path: String,
    pub json_path: String,
}

// ---------------------------------------------------------------- notifications

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Notification {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    pub task_id: Option<String>,
    pub from_member_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub deliver_after: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NudgeInput {
    pub to_member_id: String,
    #[ts(optional)]
    pub task_id: Option<String>,
    #[ts(optional)]
    pub message: Option<String>,
}

// ---------------------------------------------------------------- inbox

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Suggestion {
    pub id: String,
    pub connector_id: Option<String>,
    pub connector_name: String,
    pub subject: String,
    pub received_at: DateTime<Utc>,
    pub draft: SuggestionDraft,
    pub extractor: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decided_by: Option<String>,
    pub result_type: Option<String>,
    pub result_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AcceptSuggestionInput {
    /// "task" or "event"
    pub create_as: String,
    pub title: String,
    #[ts(optional)]
    pub notes: Option<String>,
    #[ts(optional)]
    pub date: Option<String>,
    #[ts(optional)]
    pub time: Option<String>,
    #[ts(optional)]
    pub category: Option<Category>,
    #[ts(optional)]
    pub group_id: Option<String>,
    #[ts(optional)]
    pub assignee_id: Option<String>,
    #[ts(optional)]
    pub source_id: Option<String>,
    #[ts(optional)]
    pub timezone: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IntakeInput {
    #[ts(optional)]
    pub subject: Option<String>,
    pub text: String,
    /// Use the configured AI provider (requires admin opt-in); otherwise local rules.
    #[ts(optional)]
    pub use_ai: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IntakeResult {
    pub suggestions: Vec<Suggestion>,
    pub extractor: String,
    pub notice: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectorSummary {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    pub enabled: bool,
    pub status: String,
    pub last_run_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------- admin

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Connector {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    pub enabled: bool,
    pub status: String,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub has_credentials: bool,
    pub has_cursor: bool,
    pub items_ingested: u32,
    pub ai_consent: bool,
    pub retention_days: u32,
    pub implementation: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectorInput {
    pub provider: String,
    pub display_name: String,
    #[ts(optional)]
    pub ai_consent: Option<bool>,
    #[ts(optional)]
    pub retention_days: Option<u32>,
    /// Provider-specific non-secret settings (e.g. fixture directory, IMAP host).
    #[ts(optional)]
    pub settings: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ConnectorPatch {
    #[ts(optional)]
    pub display_name: Option<String>,
    #[ts(optional)]
    pub enabled: Option<bool>,
    #[ts(optional)]
    pub ai_consent: Option<bool>,
    #[ts(optional)]
    pub retention_days: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderInfo {
    pub provider: String,
    pub name: String,
    pub implementation: String,
    pub description: String,
    pub scopes: Vec<String>,
    pub requires: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AiSettings {
    /// "none", "anthropic" or "openai_compatible".
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub has_key: bool,
    /// "env", "stored" or "none".
    pub key_source: String,
    pub max_excerpt_chars: u32,
    pub allow_paste_intake: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AiSettingsInput {
    #[ts(optional)]
    pub provider: Option<String>,
    #[ts(optional)]
    pub model: Option<String>,
    #[ts(optional)]
    pub base_url: Option<String>,
    /// Write-only. Stored encrypted server-side; never returned.
    #[ts(optional)]
    pub api_key: Option<String>,
    #[ts(optional)]
    pub clear_key: Option<bool>,
    #[ts(optional)]
    pub max_excerpt_chars: Option<u32>,
    #[ts(optional)]
    pub allow_paste_intake: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminSettings {
    pub sharing_enabled: bool,
    pub mode: ServerMode,
    pub bind: String,
    pub allowed_networks: Vec<String>,
    pub data_dir_display: String,
    pub encryption_key_source: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AdminSettingsInput {
    #[ts(optional)]
    pub sharing_enabled: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Paged<T: TS> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskHistory {
    pub activity: Vec<Activity>,
    pub completions: Vec<TaskCompletion>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TimerCommandInput {
    pub command: crate::timer::TimerCommand,
    #[ts(optional)]
    pub expected_version: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PairInput {
    pub code: String,
    #[ts(optional)]
    pub device_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SnoozeInput {
    pub minutes: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UseTemplateInput {
    #[ts(optional)]
    pub group_id: Option<String>,
    #[ts(optional)]
    pub assignee_id: Option<String>,
    #[ts(optional)]
    pub rotation: Option<Vec<String>>,
    #[ts(optional)]
    pub due_date: Option<String>,
    #[ts(optional)]
    pub timezone: Option<String>,
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub recurrence: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct TemplatePatch {
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub recurrence: Option<String>,
    #[ts(optional)]
    pub repeat_mode: Option<RepeatMode>,
    #[ts(optional)]
    pub duration_minutes: Option<u32>,
    #[ts(optional)]
    pub checklist: Option<Vec<String>>,
    #[ts(optional)]
    pub tip: Option<String>,
}
