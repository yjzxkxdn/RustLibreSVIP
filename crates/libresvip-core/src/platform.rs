//! Wall clock and process identity.
//!
//! The plugins only use these for entropy (UUIDs, colour picks, AES salts) and
//! for a couple of timestamps, but `std::time::SystemTime::now()` and
//! `std::process::id()` are not available on `wasm32-unknown-unknown`, so they
//! go through this module. The native implementations are the plain `std`
//! calls, i.e. behaviour is unchanged.

#[cfg(not(target_arch = "wasm32"))]
pub fn unix_nanos() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn unix_seconds() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn process_id() -> u32 {
    std::process::id()
}

#[cfg(target_arch = "wasm32")]
mod clock {
    use std::sync::atomic::{AtomicU64, Ordering};

    static HOST_NANOS: AtomicU64 = AtomicU64::new(0);
    static TICKS: AtomicU64 = AtomicU64::new(0);

    /// Called by the host once per conversion (or once at start-up) so
    /// timestamps and UUIDs are not all zero. Optional: without it the clock
    /// still advances, one nanosecond per call.
    pub fn set_unix_nanos(nanos: u64) {
        HOST_NANOS.store(nanos, Ordering::Relaxed);
    }

    pub fn next_nanos() -> u64 {
        let base = HOST_NANOS.load(Ordering::Relaxed);
        let tick = TICKS.fetch_add(1, Ordering::Relaxed);
        base.saturating_add(tick)
    }
}

/// `unix_nanos` for wasm: host-provided base time plus a monotonic tick, so
/// every call still yields a distinct value.
#[cfg(target_arch = "wasm32")]
pub fn unix_nanos() -> u128 {
    u128::from(clock::next_nanos())
}

#[cfg(target_arch = "wasm32")]
pub fn unix_seconds() -> u64 {
    clock::next_nanos() / 1_000_000_000
}

#[cfg(target_arch = "wasm32")]
pub fn process_id() -> u32 {
    0
}

/// Lets a wasm host seed [`unix_nanos`]; a no-op on native targets.
#[allow(unused_variables)]
pub fn set_host_unix_nanos(nanos: u64) {
    #[cfg(target_arch = "wasm32")]
    clock::set_unix_nanos(nanos);
}
