//! Cross-process serialisation for tests that launch a real browser.
//!
//! Cargo runs each integration-test file as its own process, so an in-process
//! `Mutex` only orders the tests inside one binary. Several binaries launching
//! Chromium at once on the same machine is what makes browser tests flaky:
//! launches contend, the bridge's whole-session deadline runs out, and a
//! perfectly correct assertion fails for a reason that has nothing to do with
//! the code under test.
//!
//! `create_new` is atomic across processes on every platform WVQ supports, so a
//! lock file is enough. A lock left behind by a killed process is treated as
//! stale after [`STALE_AFTER`] rather than blocking the suite forever.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// A lock older than this belonged to a process that no longer exists.
const STALE_AFTER: Duration = Duration::from_secs(300);

/// How long to wait before taking the lock anyway. A test suite that hangs is
/// worse than one that runs two browsers at once.
const MAX_WAIT: Duration = Duration::from_secs(600);

/// Held for as long as one test needs exclusive use of the browser.
pub struct BrowserLock {
    path: PathBuf,
}

impl BrowserLock {
    /// Block until no other WVQ test process is driving a browser.
    #[must_use]
    pub fn acquire() -> Self {
        let path = std::env::temp_dir().join("wvq-browser-test.lock");
        let deadline = Instant::now() + MAX_WAIT;
        loop {
            match OpenOptions::new().create_new(true).write(true).open(&path) {
                Ok(mut file) => {
                    let _ = writeln!(file, "{}", std::process::id());
                    return Self { path };
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    if is_stale(&path) || Instant::now() >= deadline {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                // An unwritable temp directory is not something a test can fix;
                // running unserialised is better than failing every browser test.
                Err(_) => return Self { path },
            }
        }
    }
}

fn is_stale(path: &std::path::Path) -> bool {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age > STALE_AFTER)
}

impl Drop for BrowserLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
