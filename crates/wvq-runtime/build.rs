//! The Playwright bridge manifest is `dist/*.js`, not a hand-maintained list.
//!
//! TypeScript compiles one ESM file per module. Rust materializes those files
//! into a temp runtime dir. A manual `BRIDGE_FILES` array silently omits a new
//! module and the Node process fails on `import`. Regenerating from `dist/`
//! keeps the closure identical to the committed JS.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dist = manifest.join("../../js/playwright-runner/dist");
    println!("cargo:rerun-if-changed={}", dist.display());

    let mut names = Vec::new();
    let entries = fs::read_dir(&dist).unwrap_or_else(|err| {
        panic!(
            "Playwright bridge dist is missing at {}: {err}",
            dist.display()
        )
    });
    for entry in entries {
        let path = entry.expect("bridge dist entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("js") {
            continue;
        }
        let name = path
            .file_name()
            .expect("bridge js file name")
            .to_string_lossy()
            .into_owned();
        println!("cargo:rerun-if-changed={}", path.display());
        names.push(name);
    }
    names.sort();
    assert!(
        !names.is_empty(),
        "Playwright bridge dist contains no .js files"
    );
    assert!(
        names.iter().any(|name| name == "main.js"),
        "bridge dist must contain main.js"
    );

    let mut code = String::from("const BRIDGE_FILES: &[(&str, &str)] = &[\n");
    for name in &names {
        let rendered = js_include_path(&dist.join(name));
        writeln!(
            code,
            "    (\"{name}\", include_str!(\"{rendered}\")),"
        )
        .expect("write generated bridge entry");
    }
    code.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("bridge_files.rs");
    fs::write(&out, code).expect("write generated bridge manifest");
}

fn js_include_path(path: &Path) -> String {
    path.to_str()
        .expect("bridge path must be UTF-8")
        .replace('\\', "/")
}
