//! `fraise doctor`: the compatibility table, measured against this machine.
//!
//! The table says which release of each tool the umbrella may talk to; `doctor` executes
//! each tool's `--version`, reads what it printed, and reports the two side by side. That is
//! all it does — the measurement is the product, so every verdict here is a fact the report
//! can name rather than a guess it had to make.
//!
//! The report is deliberately quotable: every finding carries the range that was allowed,
//! the row's reason and, when there is one, the command that installs an allowed release, so
//! a reader can act on it without opening this repository. `--json` emits the same findings
//! for a machine; Cycle 5's envelope will carry that document as its payload.
//!
//! The exit is the contract. A machine that does not satisfy the table exits with the class
//! the table names — the same class Cycle 4's guard refuses a dispatch with — and CI runs
//! `doctor` against the releases the table names, so the table is in force rather than
//! merely configured.

use std::fmt::Write as _;
use std::io::ErrorKind;
use std::process::Command;

use semver::Version;
use serde::Serialize;

use crate::compatibility::{CompatibilityTable, Tool};

/// What measuring one tool came to.
///
/// Two of these are satisfied states and four are not, which [`Verdict::satisfied`] answers;
/// the distinctions between the four exist because a reader acts differently on each, and
/// collapsing them into "not ok" is what a report must not do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Installed, and at a version the table allows.
    Ok,
    /// Installed, and at a version the table does not allow.
    OutsideTable,
    /// Not on `PATH` at all, while the table names a release to install.
    Missing,
    /// It answered, but nothing a version could be read from. Neither a version outside the
    /// table nor a missing tool: reading it as either would be a reading nobody measured.
    Unreadable,
    /// No release of it exists to require, and none is installed — the expected state for a
    /// tool the stack has not released yet.
    AwaitingRelease,
    /// No release of it exists to require, and a build is installed anyway. Nothing vouches
    /// for that version, so the guard will refuse to dispatch to it.
    Unvouched,
}

impl Verdict {
    /// Whether this verdict satisfies the table.
    #[must_use]
    pub const fn satisfied(self) -> bool {
        matches!(self, Self::Ok | Self::AwaitingRelease)
    }

    /// The verdict as a human report spells it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::OutsideTable => "outside the table",
            Self::Missing => "missing",
            Self::Unreadable => "unreadable",
            Self::AwaitingRelease => "awaiting a release",
            Self::Unvouched => "unvouched",
        }
    }
}

/// One tool's row and what measuring it came to, which is the whole of what `doctor` has to
/// say about that tool.
#[derive(Debug, Serialize)]
pub struct Finding<'a> {
    tool: &'a str,
    program: &'a str,
    allowed: Option<&'a str>,
    found: Option<String>,
    verdict: Verdict,
    why: &'a str,
    install: Option<&'a str>,
    problem: Option<String>,
}

impl Finding<'_> {
    /// The verdict, for a caller deciding rather than printing.
    #[must_use]
    pub const fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// The one-line reading: what was found, against what was allowed.
    fn reading(&self) -> String {
        let allowed = self.allowed.unwrap_or("no release to require");
        match self.verdict {
            Verdict::Ok => format!("{} — allowed {allowed}", self.found_or("a version")),
            Verdict::OutsideTable => {
                format!("{} — outside {allowed}", self.found_or("a version"))
            },
            Verdict::Missing => format!("not on PATH — allowed {allowed}"),
            Verdict::Unreadable => self
                .problem
                .clone()
                .unwrap_or_else(|| "its version could not be read".to_owned()),
            Verdict::AwaitingRelease => {
                "not installed, and no release of it exists to require".to_owned()
            },
            Verdict::Unvouched => format!(
                "{} is installed, and no release of it exists to vouch for that",
                self.found_or("a build")
            ),
        }
    }

    fn found_or(&self, fallback: &str) -> String {
        self.found.clone().unwrap_or_else(|| fallback.to_owned())
    }
}

/// Every tool the table speaks for, measured.
#[derive(Debug, Serialize)]
pub struct Report<'a> {
    // Reason: the table answers for the refusal class and is not part of the payload; the
    // findings are what a reader and Cycle 5's envelope carry.
    #[serde(skip)]
    table: &'a CompatibilityTable,
    tools: Vec<Finding<'a>>,
}

impl Report<'_> {
    /// Whether every tool satisfies the table.
    #[must_use]
    pub fn satisfied(&self) -> bool {
        self.tools.iter().all(|finding| finding.verdict.satisfied())
    }

    /// The exit for this report: zero when the table is satisfied, otherwise the exit of the
    /// class the table refuses with.
    #[must_use]
    pub fn exit(&self) -> i32 {
        if self.satisfied() {
            0
        } else {
            self.table.refusal_class().exit()
        }
    }

    /// The findings as JSON, which is what CI reads and what Cycle 5's envelope will carry.
    ///
    /// # Panics
    ///
    /// If the findings cannot be serialised, which is a bug in this module rather than a
    /// state a machine can be in.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("a finding serialises");
        json.push('\n');
        json
    }

    /// The findings as a person reads them: one line each, and for anything that does not
    /// satisfy the table, the row's reason and the command that fixes it.
    #[must_use]
    pub fn render(&self) -> String {
        let width = self.tools.iter().map(|finding| finding.tool.len()).max().unwrap_or_default();
        let labels = self
            .tools
            .iter()
            .map(|finding| finding.verdict.label().len())
            .max()
            .unwrap_or_default();

        let mut text = String::new();
        for finding in &self.tools {
            let label = finding.verdict.label();
            let _ =
                writeln!(text, "{:width$}  {label:labels$}  {}", finding.tool, finding.reading());
            if finding.verdict.satisfied() {
                continue;
            }
            // The reason is prose and is wrapped; an install command is not, because a
            // command broken across lines is one nobody can paste.
            text.push_str(&wrapped(finding.why, "      why: ", "           "));
            if let Some(install) = finding.install {
                let _ = writeln!(text, "      install: {install}");
            }
        }

        let unsatisfied = self.tools.iter().filter(|f| !f.verdict.satisfied()).count();
        let total = self.tools.len();
        if unsatisfied == 0 {
            let _ = writeln!(text, "\n{total} of {total} tools satisfy the compatibility table.");
        } else {
            let class = self.table.refusal_class();
            let _ = writeln!(
                text,
                "\n{unsatisfied} of {total} tools do not satisfy the compatibility table \
                 (exit {}, {}).",
                class.exit(),
                class.name()
            );
        }
        text
    }
}

/// Measure every tool the table speaks for.
#[must_use]
pub fn examine(table: &CompatibilityTable) -> Report<'_> {
    let tools = table.tools().map(|tool| measure(tool, probe(tool.program()))).collect();
    Report { table, tools }
}

/// What executing `<program> --version` came to.
#[derive(Debug)]
enum Probe {
    /// A version was read from the first line it printed.
    Version(Version),
    /// It is not on `PATH`.
    Missing,
    /// It answered, but with nothing a version could be read from.
    Unreadable(String),
}

/// One row, plus that row's measurement, read as a verdict.
fn measure(tool: &Tool, probe: Probe) -> Finding<'_> {
    let (verdict, found, problem) = match (tool.pins_a_release(), probe) {
        (true, Probe::Version(version)) => {
            let verdict = if tool.accepts(&version) {
                Verdict::Ok
            } else {
                Verdict::OutsideTable
            };
            (verdict, Some(version.to_string()), None)
        },
        (false, Probe::Version(version)) => (Verdict::Unvouched, Some(version.to_string()), None),
        (true, Probe::Missing) => (Verdict::Missing, None, None),
        (false, Probe::Missing) => (Verdict::AwaitingRelease, None, None),
        (_, Probe::Unreadable(problem)) => (Verdict::Unreadable, None, Some(problem)),
    };
    Finding {
        tool: tool.name(),
        program: tool.program(),
        allowed: tool.allowed(),
        found,
        verdict,
        why: tool.why(),
        install: tool.install(),
        problem,
    }
}

/// Execute `<program> --version` and read the version out of what it printed.
///
/// The umbrella never guesses a version from an exit or from silence: a program that cannot
/// say which release it is gets [`Probe::Unreadable`], and the report says what it printed
/// instead.
fn probe(program: &str) -> Probe {
    let output = match Command::new(program).arg("--version").output() {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => return Probe::Missing,
        Err(error) => {
            return Probe::Unreadable(format!("`{program} --version` could not be run: {error}"));
        },
    };
    if !output.status.success() {
        return Probe::Unreadable(format!(
            "`{program} --version` exited {:?}",
            output.status.code()
        ));
    }
    let printed = String::from_utf8_lossy(&output.stdout);
    let line = printed.lines().next().unwrap_or_default().trim();
    read_version(line).map_or_else(
        || Probe::Unreadable(format!("`{program} --version` printed {line:?}, naming no version")),
        Probe::Version,
    )
}

/// The version in a tool's `--version` line.
///
/// Three spellings are in use across the four tools — `fraiseql 2.14.1`,
/// `confiture version 1.19.0`, `fraisier, version 0.8.3` — so the rule is the first token
/// of the first line that parses as a version, rather than a pattern per tool. Only the
/// first line is read: confiture's later lines report the parser build and the native
/// extension, which are the machine's rather than the release's.
fn read_version(line: &str) -> Option<Version> {
    line.split_whitespace().find_map(|token| {
        Version::parse(token.trim_start_matches('v').trim_end_matches([',', ';'])).ok()
    })
}

/// `text` as wrapped lines, the first prefixed with `first` and the rest with `rest`.
fn wrapped(text: &str, first: &str, rest: &str) -> String {
    const WIDTH: usize = 88;

    let mut lines = String::new();
    let mut line = String::from(first);
    let mut prefix_len = first.len();
    for word in text.split_whitespace() {
        if line.len() > prefix_len && line.len() + 1 + word.len() > WIDTH {
            lines.push_str(&line);
            lines.push('\n');
            line = String::from(rest);
            prefix_len = rest.len();
        }
        if line.len() > prefix_len {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push_str(&line);
    lines.push('\n');
    lines
}

#[cfg(test)]
mod tests {
    use super::{Verdict, read_version};

    #[test]
    fn the_spellings_the_four_tools_actually_use_are_all_read() {
        // Measured on 2026-09-26 by running each binary, which is why there are three
        // shapes and not one: a per-tool pattern would be a fourth thing to keep current.
        for (printed, version) in [
            ("fraiseql 2.14.1", "2.14.1"),
            ("confiture version 1.19.0", "1.19.0"),
            ("fraisier 1.0.0-beta.11", "1.0.0-beta.11"),
            ("fraisier, version 0.8.3", "0.8.3"),
            ("specql v2.0.0", "2.0.0"),
        ] {
            let read = read_version(printed).map(|version| version.to_string());
            assert_eq!(read.as_deref(), Some(version), "reading {printed:?}");
        }
    }

    #[test]
    fn a_line_that_names_no_version_is_not_guessed_at() {
        assert_eq!(read_version("a wrapper script that forgot to say which fraisier"), None);
        assert_eq!(read_version(""), None);
    }

    #[test]
    fn only_two_verdicts_satisfy_the_table() {
        let satisfied: Vec<&str> = [
            Verdict::Ok,
            Verdict::OutsideTable,
            Verdict::Missing,
            Verdict::Unreadable,
            Verdict::AwaitingRelease,
            Verdict::Unvouched,
        ]
        .into_iter()
        .filter(|verdict| verdict.satisfied())
        .map(Verdict::label)
        .collect();
        assert_eq!(satisfied, ["ok", "awaiting a release"]);
    }
}
