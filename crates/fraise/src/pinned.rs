//! The confiture this crate measures itself against, as a test can reach it.
//!
//! Two claims in this crate are about another tool's interface rather than about our own code:
//! the vendored exit-code contract is confiture's document, and the two DSN variables
//! [`crate::dsn`] reads are names confiture's own contract defines. Both are measured against a
//! real confiture, and both **fail rather than skip** when it is absent or is a different
//! release — a guard that skips when the tool is missing is a guard that has never run, which is
//! how fraisier-core carried a table stale in eight of nine entries with both its guards green
//! (fraisier-core#63).
//!
//! The pin has one home, `tools/confiture-requirements.txt`, and this module is the one reader
//! of it.

/// The confiture release this crate is measured against, read out of
/// `tools/confiture-requirements.txt` so the pin has exactly one home.
///
/// Every test here asserts the tool it runs reports *this* version, which couples a pin bump to
/// the change it requires in the same commit — and makes "too old to have the flag" a named
/// failure instead of a silent pass.
///
/// # Panics
///
/// If the pin cannot be read or names no confiture, which is a checkout nothing here can measure.
pub fn version() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the workspace root is two levels above crates/<name>")
        .join("tools/confiture-requirements.txt");
    let pins = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading the confiture pin {}: {error}", path.display()));
    pins.lines()
        .find_map(|line| line.trim().strip_prefix("fraiseql-confiture=="))
        .unwrap_or_else(|| panic!("{} names no `fraiseql-confiture==<version>`", path.display()))
        .to_owned()
}

/// The confiture to measure: `FRAISE_CONFITURE_BIN` when set, otherwise `confiture` on `PATH`,
/// which is where CI puts the pinned one.
pub fn program() -> std::ffi::OsString {
    std::env::var_os("FRAISE_CONFITURE_BIN")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| std::ffi::OsString::from("confiture"))
}

/// Quoted in every failure that needs it, so a red checkout is three commands from green.
pub fn install_hint(pin: &str) -> String {
    format!(
        "this is measured against confiture {pin}. Install it:\n  \
         uv venv --python 3.11 /tmp/confiture\n  \
         uv pip install --python /tmp/confiture/bin/python -r \
         tools/confiture-requirements.txt\n  \
         PATH=/tmp/confiture/bin:$PATH cargo xtask ci\n\
         (CI installs the same pin and puts it on PATH before the gate.)"
    )
}

/// The pinned confiture, run with `args`, or a panic naming what to install.
///
/// The version is asserted first: an answer from another release is not a measurement of the
/// release this crate is pinned to, and taking it as one is the whole failure mode above.
///
/// # Panics
///
/// If the tool cannot be run, is a different release, or exits non-zero for `args`.
pub fn output(args: &[&str]) -> String {
    let pin = version();
    let program = program();
    let shown = program.to_string_lossy().into_owned();

    let reported = match std::process::Command::new(&program).arg("--version").output() {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned(),
        other => {
            panic!("`{shown} --version` did not answer ({other:?}).\n{}", install_hint(&pin))
        },
    };
    // `confiture --version` opens with `confiture version <semver>`; its later lines report the
    // parser build and native extension, which are the machine's rather than the release's.
    assert_eq!(
        reported,
        format!("confiture version {pin}"),
        "this is a different confiture, so what it says would be the wrong release's.\n{}",
        install_hint(&pin)
    );

    // Wide, because rich wraps this tool's help to the terminal and a name broken across two
    // lines is a name a test would not find.
    let output = std::process::Command::new(&program)
        .args(args)
        .env("COLUMNS", "400")
        .output()
        .unwrap_or_else(|error| panic!("running `{shown} {}`: {error}", args.join(" ")));
    assert!(
        output.status.success(),
        "`{shown} {}` exited {:?}",
        args.join(" "),
        output.status.code()
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}
