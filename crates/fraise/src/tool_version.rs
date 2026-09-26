//! Reading a tool's version at a process boundary.
//!
//! Every version the umbrella knows about comes from here: `fraise doctor` reads all four
//! tools to report them, and the guard on each tool boundary reads one before it dispatches.
//! One reader, because two would be two answers to the same question — fraisier-core carried
//! a pair of exit-table guards that compared different things and agreed only by luck
//! (fraisier-core#63).
//!
//! Nothing here guesses. A program that cannot say which release it is produces
//! [`Reading::Unreadable`] carrying what it printed instead, never a version inferred from
//! its exit or from silence.

use std::io::ErrorKind;
use std::process::Command;

use semver::Version;

/// What executing `<program> --version` came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// The version read from the first line it printed.
    Version(Version),
    /// It is not on `PATH`.
    Missing,
    /// It answered, but with nothing a version could be read from. The string says what was
    /// read instead, so a report can quote it.
    Unreadable(String),
}

/// Execute `<program> --version` and read the version out of what it printed.
#[must_use]
pub fn read(program: &str) -> Reading {
    let output = match Command::new(program).arg("--version").output() {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => return Reading::Missing,
        Err(error) => {
            return Reading::Unreadable(format!("`{program} --version` could not be run: {error}"));
        },
    };
    if !output.status.success() {
        return Reading::Unreadable(format!(
            "`{program} --version` exited {:?}",
            output.status.code()
        ));
    }
    let printed = String::from_utf8_lossy(&output.stdout);
    let line = printed.lines().next().unwrap_or_default().trim();
    version_in(line).map_or_else(
        || {
            Reading::Unreadable(format!(
                "`{program} --version` printed {line:?}, naming no version"
            ))
        },
        Reading::Version,
    )
}

/// The version in a tool's `--version` line.
///
/// Three spellings are in use across the four tools — `fraiseql 2.14.1`,
/// `confiture version 1.19.0`, `fraisier, version 0.8.3` — so the rule is the first token of
/// the first line that parses as a version, rather than a pattern per tool that would be a
/// fourth thing to keep current. Only the first line is read: confiture's later lines report
/// the parser build and the native extension, which are the machine's rather than the
/// release's.
fn version_in(line: &str) -> Option<Version> {
    line.split_whitespace().find_map(|token| {
        Version::parse(token.trim_start_matches('v').trim_end_matches([',', ';'])).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::{Reading, read, version_in};

    #[test]
    fn the_spellings_the_four_tools_actually_use_are_all_read() {
        // Measured on 2026-09-26 by running each binary, which is why there are three shapes
        // and not one.
        for (printed, version) in [
            ("fraiseql 2.14.1", "2.14.1"),
            ("confiture version 1.19.0", "1.19.0"),
            ("fraisier 1.0.0-beta.11", "1.0.0-beta.11"),
            ("fraisier, version 0.8.3", "0.8.3"),
            ("specql v2.0.0", "2.0.0"),
        ] {
            let read = version_in(printed).map(|version| version.to_string());
            assert_eq!(read.as_deref(), Some(version), "reading {printed:?}");
        }
    }

    #[test]
    fn a_line_that_names_no_version_is_not_guessed_at() {
        assert_eq!(version_in("a wrapper script that forgot to say which fraisier"), None);
        assert_eq!(version_in(""), None);
    }

    #[test]
    fn a_program_that_is_not_installed_is_missing_rather_than_unreadable() {
        // The distinction the report and the guard both act on, so it is asserted against a
        // real lookup failure rather than trusted to the `ErrorKind` arm above.
        assert_eq!(read("fraise-no-such-program-exists"), Reading::Missing);
    }
}
