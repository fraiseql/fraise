//! `fraise doctor` is the compatibility table in force.
//!
//! The four tools release on four schedules, so the umbrella states which release of
//! each one it is allowed to talk to and `doctor` measures the machine against that
//! statement. The measurement has to be the real one — exec the program, read what it
//! prints — so these tests put stub programs on a temporary `PATH` and assert both what
//! is reported and the exit, which is what Cycle 4's guard will refuse on.
//!
//! The version strings below are the formats the three installable tools actually print
//! (`confiture version 1.19.0`, `fraiseql 2.14.1`, `fraisier 1.0.0-beta.11`): three
//! spellings, measured on 2026-09-26, not invented here.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The exit `fraise` refuses with when the table is not satisfied. It is confiture's
/// `precondition_failed`, and it is written here as the integer the frozen contract gives
/// that class so this file names no class of its own — the same reason the exit table's
/// tests state their mapping in integers.
const REFUSED: i32 = 2;

/// A directory of stub programs, each printing what a real tool prints for `--version`.
///
/// `CARGO_TARGET_TMPDIR` is cargo's own scratch space for integration tests, so the stubs
/// land under `target/` and never in the user's temp directory.
fn stub_path(case: &str, programs: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("doctor").join(case);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the stub directory is created");
    for (program, version_output) in programs {
        let path = dir.join(program);
        // `echo` and nothing else: `PATH` holds only this directory when the stubs run, so a
        // stub that shelled out to `cat` would exit 127 instead of printing a version.
        let mut script = String::from("#!/bin/sh\n");
        for line in version_output.lines() {
            script.push_str("echo '");
            script.push_str(line);
            script.push_str("'\n");
        }
        fs::write(&path, script).expect("the stub is written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("the stub is made executable");
    }
    dir
}

/// `fraise doctor`, seeing only the stubs: `PATH` is replaced rather than extended, so a
/// tool installed on the machine running the tests cannot answer for one of the stubs.
fn doctor(path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fraise"))
        .arg("doctor")
        .args(args)
        .env("PATH", path)
        .output()
        .expect("the built binary runs")
}

/// The report as JSON, which is the form CI reads: the payload of the one envelope every
/// command answers in under `--json`.
fn report(output: &Output) -> serde_json::Value {
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "`fraise doctor --json` did not emit JSON ({error}); stdout: {}; stderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });
    envelope["payload"].clone()
}

/// One tool's finding, by the tool's name rather than by position.
fn finding<'a>(report: &'a serde_json::Value, tool: &str) -> &'a serde_json::Value {
    report["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("the report's `tools` is an array: {report}"))
        .iter()
        .find(|finding| finding["tool"] == tool)
        .unwrap_or_else(|| panic!("the report says nothing about {tool}: {report}"))
}

fn shown(output: &Output) -> String {
    format!(
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The three installable tools at the releases the table pins, which is the state CI
/// installs. specql is absent because no release of it exists to install — the table says
/// so, and that is a pass rather than a hole.
#[test]
fn the_stack_at_its_pinned_releases_is_reported_ok_and_exits_zero() {
    let path = stub_path(
        "allowed",
        &[
            // Confiture's later lines report the parser build and the native extension,
            // which are the machine's rather than the release's; only the first line is
            // the version, and a stub that prints more is how that is proven.
            ("confiture", "confiture version 1.19.0\nparser: native\nextension: enabled"),
            ("fraiseql", "fraiseql 2.14.1"),
            ("fraisier", "fraisier 1.0.0-beta.11"),
        ],
    );

    let output = doctor(&path, &["--json"]);
    assert!(
        output.status.success(),
        "the pinned stack should satisfy the table: {}",
        shown(&output)
    );

    let report = report(&output);
    for (tool, version) in [
        ("confiture", "1.19.0"),
        ("fraiseql", "2.14.1"),
        ("fraisier", "1.0.0-beta.11"),
    ] {
        let finding = finding(&report, tool);
        assert_eq!(finding["verdict"], "ok", "{tool}: {finding}");
        assert_eq!(finding["found"], version, "{tool} reports the version it printed");
        assert!(finding["allowed"].is_string(), "{tool}'s allowed range is reported: {finding}");
    }
    let specql = finding(&report, "specql");
    assert_eq!(
        specql["verdict"], "awaiting_release",
        "specql has no release to require: {specql}"
    );
    assert_eq!(specql["found"], serde_json::Value::Null);
    assert_eq!(specql["allowed"], serde_json::Value::Null);
}

/// A version the table does not allow is the case the table exists for: fraiseql's newest
/// release is 2.14.1, so its predecessor is outside it. The report has to name the version
/// it found *and* what was allowed, or the reader cannot act on it.
#[test]
fn a_version_outside_the_table_is_named_with_what_was_allowed_and_refused() {
    let path = stub_path(
        "outside",
        &[
            ("confiture", "confiture version 1.19.0"),
            ("fraiseql", "fraiseql 2.13.0"),
            ("fraisier", "fraisier 1.0.0-beta.11"),
        ],
    );

    let output = doctor(&path, &["--json"]);
    assert_eq!(
        output.status.code(),
        Some(REFUSED),
        "an out-of-table version is a failed precondition: {}",
        shown(&output)
    );

    let report = report(&output);
    let fraiseql = finding(&report, "fraiseql");
    assert_eq!(fraiseql["verdict"], "outside_table", "{fraiseql}");
    assert_eq!(fraiseql["found"], "2.13.0");
    assert!(
        fraiseql["allowed"].as_str().is_some_and(|allowed| allowed.contains("2.14.1")),
        "the range it failed is reported: {fraiseql}"
    );
    assert_eq!(finding(&report, "confiture")["verdict"], "ok", "one bad tool fails only itself");
}

/// A tool that is not installed at all, reported as missing with the command that installs
/// it — the report is meant to be actionable without opening this repository.
#[test]
fn a_tool_that_is_not_installed_is_reported_missing_with_how_to_install_it() {
    let path = stub_path(
        "missing",
        &[
            ("confiture", "confiture version 1.19.0"),
            ("fraiseql", "fraiseql 2.14.1"),
        ],
    );

    let output = doctor(&path, &["--json"]);
    assert_eq!(
        output.status.code(),
        Some(REFUSED),
        "a missing tool is a failed precondition: {}",
        shown(&output)
    );

    let report = report(&output);
    let fraisier = finding(&report, "fraisier");
    assert_eq!(fraisier["verdict"], "missing", "{fraisier}");
    assert_eq!(fraisier["found"], serde_json::Value::Null);
    assert!(
        fraisier["install"].is_string(),
        "a tool that can be installed says how: {fraisier}"
    );

    let human = doctor(&path, &[]);
    let text = String::from_utf8_lossy(&human.stdout).into_owned();
    assert!(text.contains("fraisier"), "the human report names the tool: {text}");
    assert!(
        fraisier["install"].as_str().is_some_and(|install| text.contains(install)),
        "the human report carries the install command too: {text}"
    );
}

/// specql publishes no release `fraise` can name, so its absence passes — but a build of it
/// on `PATH` is a version nothing vouches for, and Cycle 4's guard will refuse to dispatch
/// to it. `doctor` says so rather than reporting a green machine.
#[test]
fn a_tool_with_no_pinned_release_is_refused_when_one_is_installed_anyway() {
    let path = stub_path(
        "unvouched",
        &[
            ("confiture", "confiture version 1.19.0"),
            ("fraiseql", "fraiseql 2.14.1"),
            ("fraisier", "fraisier 1.0.0-beta.11"),
            ("specql", "specql 2.0.0"),
        ],
    );

    let output = doctor(&path, &["--json"]);
    assert_eq!(
        output.status.code(),
        Some(REFUSED),
        "a build the table cannot vouch for is a failed precondition: {}",
        shown(&output)
    );

    let report = report(&output);
    let specql = finding(&report, "specql");
    assert_eq!(specql["verdict"], "unvouched", "{specql}");
    assert_eq!(specql["found"], "2.0.0", "what is installed is still reported: {specql}");
    assert!(
        specql["why"].is_string(),
        "the table's reason travels with the finding: {specql}"
    );
}

/// A program that answers but says nothing a version can be read from. It is not a version
/// outside the table and not a missing tool: guessing either would be a reading nobody
/// measured, so it is its own verdict and it refuses.
#[test]
fn a_tool_whose_version_cannot_be_read_is_its_own_verdict_and_refuses() {
    let path = stub_path(
        "unreadable",
        &[
            ("confiture", "confiture version 1.19.0"),
            ("fraiseql", "fraiseql 2.14.1"),
            ("fraisier", "a wrapper script that forgot to say which fraisier"),
        ],
    );

    let output = doctor(&path, &["--json"]);
    assert_eq!(
        output.status.code(),
        Some(REFUSED),
        "an unreadable version refuses: {}",
        shown(&output)
    );

    let report = report(&output);
    let fraisier = finding(&report, "fraisier");
    assert_eq!(fraisier["verdict"], "unreadable", "{fraisier}");
    assert!(
        fraisier["problem"].is_string(),
        "an unreadable version says what was read instead: {fraisier}"
    );
}
