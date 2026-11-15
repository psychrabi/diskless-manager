//! Emits the compile-time build stamp surfaced by the GUI and `version` verb.

use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    println!("cargo:rustc-env=DM_BUILD_UNIX={unix_seconds}");
}
