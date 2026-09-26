//! `fraise --version` is the umbrella's first contract: whatever it dispatches
//! to, it can always say which `fraise` is speaking. Every later cycle reports
//! the four tools' versions, so this one proves it reports its own.

use std::process::Command;

// Running the built binary is the only way to observe what a user observes:
// the version string is rendered by the argument parser, not by our code, so
// asserting on a library constant would test nothing.
#[test]
fn the_binary_prints_its_own_crate_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_fraise"))
        .arg("--version")
        .output()
        .expect("the built binary runs");

    assert!(
        output.status.success(),
        "`fraise --version` exited with {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");
    assert_eq!(stdout.trim(), format!("fraise {}", env!("CARGO_PKG_VERSION")));
}
