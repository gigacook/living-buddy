use crate::config::Config;
use crate::crypto::Cipher;
use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState(pub Arc<Inner>);

pub struct Inner {
    pub db: SqlitePool,
    pub config: Config,
    pub cipher: Cipher,
    pub limiter: RateLimiter,
    pub http: reqwest::Client,
    pub clock: Clock,
}

impl std::ops::Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl AppState {
    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }
}

/// Wall clock with an adjustable offset so tests can simulate sleep/suspend.
#[derive(Default)]
pub struct Clock {
    offset_ms: AtomicI64,
}

impl Clock {
    pub fn now(&self) -> DateTime<Utc> {
        Utc::now() + Duration::milliseconds(self.offset_ms.load(Ordering::Relaxed))
    }
    pub fn advance(&self, d: Duration) {
        self.offset_ms.fetch_add(d.num_milliseconds(), Ordering::Relaxed);
    }
}

/// Small fixed-window limiter keyed by (bucket, ip). In-memory and per process.
#[derive(Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<(&'static str, IpAddr), (i64, u32)>>,
}

impl RateLimiter {
    pub fn check(&self, bucket: &'static str, ip: IpAddr, limit: u32, window_secs: i64, now: DateTime<Utc>) -> bool {
        let slot = now.timestamp() / window_secs;
        let mut map = self.windows.lock().expect("limiter lock");
        if map.len() > 50_000 {
            map.retain(|_, (s, _)| *s == slot);
        }
        let entry = map.entry((bucket, ip)).or_insert((slot, 0));
        if entry.0 != slot {
            *entry = (slot, 0);
        }
        entry.1 += 1;
        entry.1 <= limit
    }
}
