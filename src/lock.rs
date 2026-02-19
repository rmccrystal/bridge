use std::fs::{self, File};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use fs2::FileExt;

/// Guard that holds an exclusive file lock. The lock is released when dropped.
pub struct LockGuard {
    _file: File,
}

/// Acquire an exclusive lock for the given hostname and lock name.
///
/// Lock file is stored at `/tmp/bridge-{hostname}-{lock_name}.lock`.
/// If the lock is already held, polls every 2 seconds until acquired or timeout.
pub fn acquire_lock(
    hostname: &str,
    lock_name: &str,
    timeout: Duration,
    verbose: bool,
) -> Result<LockGuard> {
    let lock_path = format!("/tmp/bridge-{}-{}.lock", hostname, lock_name);

    // Ensure the lock file exists
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("Failed to open lock file: {}", lock_path))?;

    // Try non-blocking lock first
    if file.try_lock_exclusive().is_ok() {
        if verbose {
            eprintln!("Acquired lock '{}' on {}", lock_name, hostname);
        }
        return Ok(LockGuard { _file: file });
    }

    eprintln!(
        "Waiting for lock '{}' on {}...",
        lock_name, hostname
    );

    let start = Instant::now();
    let poll_interval = Duration::from_secs(2);

    loop {
        if start.elapsed() >= timeout {
            anyhow::bail!(
                "Timed out waiting for lock '{}' on {} after {}s",
                lock_name,
                hostname,
                timeout.as_secs()
            );
        }

        thread::sleep(poll_interval);

        if file.try_lock_exclusive().is_ok() {
            eprintln!("Acquired lock '{}' on {}", lock_name, hostname);
            return Ok(LockGuard { _file: file });
        }
    }
}
