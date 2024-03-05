pub mod fri_gpu_prover_queue_dal;
pub mod fri_proof_compressor_dal;
pub mod fri_protocol_versions_dal;
pub mod fri_prover_dal;
pub mod fri_scheduler_dependency_tracker_dal;
pub mod fri_witness_generator_dal;

use std::time::Duration;

use chrono::NaiveTime;
use sqlx::postgres::types::PgInterval;

pub const fn pg_interval_from_duration(processing_timeout: Duration) -> PgInterval {
    PgInterval {
        months: 0,
        days: 0,
        microseconds: processing_timeout.as_micros() as i64,
    }
}

pub fn duration_to_naive_time(duration: Duration) -> NaiveTime {
    let total_seconds = duration.as_secs() as u32;
    NaiveTime::from_hms_opt(
        total_seconds / 3600,
        (total_seconds / 60) % 60,
        total_seconds % 60,
    )
    .unwrap()
}
