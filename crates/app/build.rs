//! Embeds the app icon into `timberstat.exe` on Windows. GPUI loads icon resource 1 for the
//! window and taskbar; Explorer uses the same resource for the file icon.

use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let icon = manifest_dir.join("../../assets/favicon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon = icon.canonicalize().expect("assets/favicon.ico exists");
    // rc.exe takes the path as a C string literal: forward slashes avoid escaping, and the
    // `\\?\` prefix from canonicalize is stripped.
    let icon = icon.display().to_string().replace('\\', "/");
    let icon = icon.trim_start_matches("//?/");
    let rc = PathBuf::from(env::var("OUT_DIR").expect("set by cargo")).join("timberstat.rc");
    fs::write(&rc, format!("1 ICON \"{icon}\"\n")).expect("write timberstat.rc");
    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_optional()
        .expect("compile the icon resource");
}
