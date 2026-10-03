//! Tendly core: business rules shared by the HTTP server, the worker and the
//! native shell. Nothing in this crate performs I/O.

pub mod alarm;
pub mod api;
pub mod extraction;
pub mod ics;
pub mod merge;
pub mod model;
pub mod notify;
pub mod recurrence;
pub mod redact;
pub mod rotation;
pub mod share;
pub mod templates;
pub mod timer;
pub mod usage;

pub use model::*;
