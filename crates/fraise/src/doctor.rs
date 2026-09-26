//! `fraise doctor`: the compatibility table, measured against this machine.
//!
//! The table says which release of each tool the umbrella may talk to; `doctor` reads each
//! tool's version with [`crate::tool_version`], asks the tool's own row what that means with
//! [`Tool::judge`], and reports the two side by side. It owns no rule of its own about what a
//! reading means — the guard on every tool boundary will ask the same row the same question,
//! and a report that disagreed with the guard would be worse than no report.
//!
//! What it does own is making the answer actionable: every finding carries the range that was
//! allowed, the row's reason and, when there is one, the command that installs an allowed
//! release, so a reader can act without opening this repository. `--json` emits the same
//! findings for a machine, as the payload of the one envelope every command answers in.
//!
//! The exit is the contract. A machine that does not satisfy the table exits with the class
//! the table names, and CI installs the releases the table names — by reading the table — and
//! requires `doctor` to accept them, so the table is in force rather than merely configured.

use std::fmt::Write as _;

use serde::Serialize;

use crate::compatibility::{CompatibilityTable, Tool, Verdict};
use crate::envelope::Payload;
use crate::tool_version;

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
            Verdict::OutsideTable => format!("{} — outside {allowed}", self.found_or("a version")),
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

    /// The findings as the envelope's payload: a document, because `fraise` produced it
    /// itself and has no tool's word to take for it. This is what CI reads the install
    /// commands out of.
    ///
    /// # Panics
    ///
    /// If the findings cannot be serialised, which is a bug in this module rather than a state
    /// a machine can be in.
    #[must_use]
    pub fn payload(&self) -> Payload {
        Payload::Json(serde_json::to_value(self).expect("a finding serialises"))
    }

    /// The findings as a person reads them: one line each, and for anything that does not
    /// satisfy the table, the row's reason and the command that fixes it.
    #[must_use]
    pub fn render(&self) -> String {
        let tools = self.tools.iter().map(|finding| finding.tool.len()).max().unwrap_or_default();
        let verdicts = self
            .tools
            .iter()
            .map(|finding| label(finding.verdict).len())
            .max()
            .unwrap_or_default();

        let mut text = String::new();
        for finding in &self.tools {
            let verdict = label(finding.verdict);
            let _ = writeln!(
                text,
                "{:tools$}  {verdict:verdicts$}  {}",
                finding.tool,
                finding.reading()
            );
            if finding.verdict.satisfied() {
                continue;
            }
            // The reason is prose and is wrapped; an install command is not, because a command
            // broken across lines is one nobody can paste.
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
    let tools = table.tools().map(measure).collect();
    Report { table, tools }
}

/// One row, measured on this machine and judged by that row.
fn measure(tool: &Tool) -> Finding<'_> {
    let reading = tool_version::read(tool.program());
    let verdict = tool.judge(&reading);
    let (found, problem) = match reading {
        tool_version::Reading::Version(version) => (Some(version.to_string()), None),
        tool_version::Reading::Missing => (None, None),
        tool_version::Reading::Unreadable(problem) => (None, Some(problem)),
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

/// The verdict as a human report spells it. Presentation, which is why it lives here and the
/// verdict itself lives with the table.
const fn label(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Ok => "ok",
        Verdict::OutsideTable => "outside the table",
        Verdict::Missing => "missing",
        Verdict::Unreadable => "unreadable",
        Verdict::AwaitingRelease => "awaiting a release",
        Verdict::Unvouched => "unvouched",
    }
}

/// `text` as wrapped lines, the first prefixed with `first` and the rest with `rest`.
fn wrapped(text: &str, first: &str, rest: &str) -> String {
    const WIDTH: usize = 88;

    let mut lines = String::new();
    let mut line = String::from(first);
    let mut prefix = first.len();
    for word in text.split_whitespace() {
        if line.len() > prefix && line.len() + 1 + word.len() > WIDTH {
            lines.push_str(&line);
            lines.push('\n');
            line = String::from(rest);
            prefix = rest.len();
        }
        if line.len() > prefix {
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
    use super::{examine, label};
    use crate::compatibility::{CompatibilityTable, Verdict};

    #[test]
    fn every_verdict_a_report_can_carry_has_a_word_for_it() {
        // `label` is exhaustive by the compiler; what this asserts is that none of the words
        // is empty or shared, since the human report distinguishes findings by them alone.
        let words: Vec<&str> = [
            Verdict::Ok,
            Verdict::OutsideTable,
            Verdict::Missing,
            Verdict::Unreadable,
            Verdict::AwaitingRelease,
            Verdict::Unvouched,
        ]
        .into_iter()
        .map(label)
        .collect();
        assert!(words.iter().all(|word| !word.is_empty()));
        let mut unique = words.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), words.len(), "two verdicts read the same: {words:?}");
    }

    #[test]
    fn a_report_speaks_for_every_row_of_the_table() {
        // Whatever is installed on the machine running this, the report is not allowed to be
        // shorter than the table: a tool that went unmeasured would read as a clean bill.
        let table = CompatibilityTable::vendored();
        let report = examine(table);
        let json = serde_json::to_string(&report).expect("a report serialises");
        for tool in table.tools() {
            assert!(
                json.contains(tool.name()),
                "{} is missing from the report: {json}",
                tool.name()
            );
        }
        assert_eq!(report.satisfied(), report.exit() == 0);
    }
}
