pub mod admin;
pub mod calendar;
pub mod focus;
pub mod groups;
pub mod inbox;
pub mod members;
pub mod misc;
pub mod session;
pub mod shares;
pub mod tasks;

use crate::error::AppError;
use crate::state::AppState;
use axum::extract::DefaultBodyLimit;
use axum::response::IntoResponse;
use axum::routing::{any, get, patch, post};
use axum::Router;

async fn api_not_found() -> impl IntoResponse {
    AppError::NotFound("endpoint")
}

pub fn api(state: AppState) -> Router {
    let admin = Router::new()
        .route("/api/admin/settings", get(admin::get_settings).patch(admin::patch_settings))
        .route("/api/admin/ai", get(admin::get_ai).put(admin::put_ai))
        .route("/api/admin/ai/test", post(admin::ai_test))
        .route("/api/admin/connectors/providers", get(admin::providers))
        .route("/api/admin/connectors", get(admin::list_connectors).post(admin::create_connector))
        .route("/api/admin/connectors/{id}", patch(admin::patch_connector).delete(admin::delete_connector))
        .route("/api/admin/connectors/{id}/run", post(admin::run_now))
        .route("/api/admin/connectors/{id}/disconnect", post(admin::disconnect))
        .route("/api/admin/connectors/{id}/credentials", post(admin::set_credentials))
        .route("/api/admin/oauth/callback", get(admin::oauth_callback))
        .route("/api/admin/oauth/{provider}/start", get(admin::oauth_start))
        .route("/api/admin/jobs", get(admin::jobs))
        .route("/api/admin/devices", get(admin::list_devices).post(admin::create_device))
        .route("/api/admin/devices/{id}/revoke", post(admin::revoke_device))
        .route("/api/admin/export", get(admin::export));

    let import = Router::new()
        .route("/api/calendar/import", post(calendar::import))
        .layer(DefaultBodyLimit::max(6 * 1024 * 1024));

    Router::new()
        .route("/healthz", get(session::healthz))
        .route("/readyz", get(session::readyz))
        .route("/api/session", get(session::session))
        .route("/api/auth/pair", post(session::pair))
        .route("/api/auth/unpair", post(session::unpair))
        .route("/api/members", get(members::list).post(members::create))
        .route("/api/members/{id}", patch(members::update))
        .route("/api/groups", get(groups::list).post(groups::create))
        .route("/api/groups/{id}", get(groups::get).patch(groups::update))
        .route("/api/groups/{id}/join", post(groups::join))
        .route("/api/groups/{id}/leave", post(groups::leave))
        .route("/api/groups/{id}/milestones", post(groups::add_milestone))
        .route("/api/groups/{id}/milestones/{mid}", patch(groups::update_milestone).delete(groups::delete_milestone))
        .route("/api/tasks", get(tasks::list).post(tasks::create))
        .route("/api/tasks/{id}", get(tasks::get).patch(tasks::update).delete(tasks::delete))
        .route("/api/tasks/{id}/complete", post(tasks::complete))
        .route("/api/tasks/{id}/reopen", post(tasks::reopen))
        .route("/api/tasks/{id}/move", post(tasks::move_task))
        .route("/api/tasks/{id}/history", get(tasks::history))
        .route("/api/templates", get(misc::templates))
        .route("/api/templates/{key}", patch(misc::update_template))
        .route("/api/templates/{key}/use", post(misc::use_template))
        .route("/api/activity", get(misc::activity))
        .route("/api/notifications", get(misc::notifications))
        .route("/api/notifications/read-all", post(misc::mark_all_read))
        .route("/api/notifications/{id}/read", post(misc::mark_read))
        .route("/api/nudges", post(misc::nudge))
        .route("/api/timer", get(focus::get_timer).post(focus::timer_command))
        .route("/api/countdowns", get(focus::countdowns).post(focus::create_countdown))
        .route("/api/countdowns/{id}", axum::routing::delete(focus::delete_countdown))
        .route("/api/alarms", get(focus::alarms).post(focus::create_alarm))
        .route("/api/alarms/{id}", patch(focus::update_alarm).delete(focus::delete_alarm))
        .route("/api/alarms/{id}/fired", post(focus::alarm_fired))
        .route("/api/alarms/{id}/snooze", post(focus::alarm_snooze))
        .route("/api/usage", get(focus::usage).put(focus::put_usage))
        .route("/api/calendar/sources", get(calendar::sources).post(calendar::create_source))
        .route("/api/calendar/sources/{id}", patch(calendar::update_source).delete(calendar::delete_source))
        .route("/api/calendar/sources/{id}/refresh", post(calendar::refresh_source))
        .route("/api/calendar/occurrences", get(calendar::occurrences))
        .route("/api/calendar/export.ics", get(calendar::export_ics))
        .route("/api/calendar/export.json", get(calendar::export_json))
        .route("/api/calendar/events", post(calendar::create_event))
        .route("/api/calendar/events/{id}", get(calendar::get_event).patch(calendar::update_event))
        .route("/api/calendar/changes", get(calendar::changes))
        .route("/api/shares", get(shares::list).post(shares::create))
        .route("/api/shares/{id}/revoke", post(shares::revoke))
        .route("/api/inbox/suggestions", get(inbox::list))
        .route("/api/inbox/suggestions/{id}/accept", post(inbox::accept))
        .route("/api/inbox/suggestions/{id}/dismiss", post(inbox::dismiss))
        .route("/api/inbox/intake", post(inbox::intake))
        .route("/api/inbox/connectors", get(inbox::connectors))
        .route("/api/inbox/count", get(inbox::counts))
        .route("/share/{token}", get(shares::public_html))
        .route("/share/{token}/calendar.ics", get(shares::public_ics))
        .route("/share/{token}/calendar.json", get(shares::public_json))
        .merge(admin)
        .merge(import)
        .route("/api/{*rest}", any(api_not_found))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(state)
}
