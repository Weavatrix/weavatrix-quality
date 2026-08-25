//! Command-bus unit tests.

use super::*;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("wvq-{label}-{nanos}"));
        std::fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn record(executor: &str) -> ExecutorRecord {
    ExecutorRecord {
        executor: executor.into(),
        cwd: ".".into(),
        selection: Vec::new(),
        status_code: Some(0),
        passed: true,
        error: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
        artifacts: Vec::new(),
    }
}

mod analytics;
mod artifacts;
mod paths;
mod policy;
mod protection;
