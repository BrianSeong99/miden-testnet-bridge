use std::time::Duration;

use anyhow::{Context, Result};
use time::OffsetDateTime;
use tokio::time::MissedTickBehavior;
use tracing::{error, warn};

use crate::core::{
    lifecycle::{DynLifecycle, LifecycleEvent},
    state::DynStateStore,
};

const PROCESSING_STUCK_AFTER_SECS: i64 = 30 * 60;

pub fn spawn_deadline_expiry_scanner(
    store: DynStateStore,
    lifecycle: DynLifecycle,
    interval: Duration,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            if let Err(error) = run_deadline_expiry_tick(store.clone(), lifecycle.clone()).await {
                error!(error = %error, "deadline expiry scan failed");
            }
        }
    });
}

pub fn spawn_stuck_processing_scanner(store: DynStateStore, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            if let Err(error) = run_stuck_processing_scan_tick(store.clone()).await {
                error!(error = %error, "stuck processing scan failed");
            }
        }
    });
}

pub async fn run_deadline_expiry_tick(
    store: DynStateStore,
    lifecycle: DynLifecycle,
) -> Result<usize> {
    let expired_quotes = store
        .list_deadline_expired_quotes(OffsetDateTime::now_utc())
        .await
        .context("failed to list deadline-expired quotes")?;

    for quote in &expired_quotes {
        lifecycle
            .apply(LifecycleEvent::DeadlineExpired {
                correlation_id: quote.correlation_id,
            })
            .await
            .with_context(|| {
                format!(
                    "failed to apply deadline expiry for {}",
                    quote.correlation_id
                )
            })?;
    }

    Ok(expired_quotes.len())
}

pub async fn run_stuck_processing_scan_tick(store: DynStateStore) -> Result<usize> {
    let cutoff = OffsetDateTime::now_utc() - time::Duration::seconds(PROCESSING_STUCK_AFTER_SECS);
    let stuck_quotes = store
        .list_stuck_processing_quotes(cutoff)
        .await
        .context("failed to list stuck processing quotes")?;

    for quote in &stuck_quotes {
        warn!(
            correlation_id = %quote.correlation_id,
            status = %quote.status,
            updated_at = %format_timestamp(quote.updated_at),
            last_event_at = quote.last_event_at.map(format_timestamp),
            "processing quote exceeds watchdog threshold"
        );
    }

    Ok(stuck_quotes.len())
}

fn format_timestamp(value: OffsetDateTime) -> String {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting should succeed")
}
