//! Deadline/cancel must kill descendants, not just the parent.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wvq_runtime::{ProcessLimits, RuntimeError, run_bounded};

fn marker() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("wvq-tree-kill-{nanos}.log"))
}

fn node_program() -> &'static str {
    if cfg!(windows) { "node.exe" } else { "node" }
}

/// Parent Node keeps a grandchild writing to `file` until the tree is killed.
fn tree_script() -> &'static str {
    r#"
const {spawn}=require('child_process');
const file=process.argv[1];
const child=spawn(process.execPath,['-e','setInterval(()=>require("fs").appendFileSync(process.argv[1],"x"),40)',file],{stdio:'ignore'});
child.unref();
setInterval(()=>{},1000);
"#
}

#[test]
fn a_deadline_kills_the_grandchild_not_only_the_parent() {
    let file = marker();
    let _ = std::fs::remove_file(&file);
    let err = run_bounded(
        node_program(),
        &["-e".into(), tree_script().into(), file.display().to_string()],
        std::env::temp_dir().as_path(),
        &ProcessLimits {
            deadline: Duration::from_millis(250),
            max_output_bytes: 64 * 1024,
        },
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(err, RuntimeError::DeadlineExceeded), "{err:?}");

    std::thread::sleep(Duration::from_millis(200));
    let first = std::fs::metadata(&file).map(|meta| meta.len()).unwrap_or(0);
    std::thread::sleep(Duration::from_millis(250));
    let second = std::fs::metadata(&file).map(|meta| meta.len()).unwrap_or(0);
    let _ = std::fs::remove_file(&file);
    assert_eq!(
        first, second,
        "grandchild kept writing after the parent was killed"
    );
}
