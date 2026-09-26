//! The guard on every tool boundary.
//!
//! `doctor` reports the compatibility table; this is the table in force. Nothing crosses into
//! a tool without its version being read and judged first, and the judgement is
//! [`crate::compatibility::Tool::judge`] — the same one the report prints, so a machine
//! `doctor` calls green cannot be one the guard refuses, and the reverse.
//!
//! Two properties are held by the types rather than by care. A [`Cleared`] is the only thing
//! [`Dispatcher::run`] accepts and only [`Dispatcher::clear`] hands one out, so there is no
//! spelling of "dispatch without checking". And the working directory of the child is a field
//! of the dispatcher, never inherited by accident: fraiseql#1387 is `compile` reading
//! `fraiseql.toml` from the working directory, which from a sibling directory emitted 0 unions
//! of 94 at exit 0 — a tool run in the wrong place can succeed at doing nothing.
//!
//! A skew the table does not allow can be tolerated on purpose
//! (`--allow-version-skew`, `FRAISE_ALLOW_VERSION_SKEW`), because a stack is sometimes mid-
//! upgrade and refusing to work is not always the kinder answer. What it will not do is
//! tolerate it quietly: [`Cleared::tolerated`] carries what was let through, for the caller to
//! say out loud and for the envelope to record as a field. The hatch covers a version the table
//! disagrees with, never a tool that is absent or a version that could not be read — those are
//! not skew, and pretending to tolerate them would be inventing permission nobody gave.

use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use serde::Serialize;

use crate::compatibility::{CompatibilityTable, Tool, Verdict};
use crate::envelope::{Asked, Payload};
use crate::exit_table::{ExitClass, ExitTable};
use crate::tool_version::{self, Reading};

/// Whether a version the table does not allow may proceed anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tolerance {
    /// Refuse it, which is the default and the promise the umbrella makes.
    Refuse,
    /// Proceed, and report what was tolerated.
    TolerateSkew,
}

/// The one way into a tool.
#[derive(Debug)]
pub struct Dispatcher<'a> {
    table: &'a CompatibilityTable,
    contract: &'static ExitTable,
    directory: PathBuf,
    tolerance: Tolerance,
}

impl<'a> Dispatcher<'a> {
    /// A dispatcher that runs tools in `directory`.
    ///
    /// The directory is the caller's decision and is passed to every child explicitly, so a
    /// tool that reads its configuration from the working directory reads the one the caller
    /// meant.
    #[must_use]
    pub fn new(table: &'a CompatibilityTable, directory: PathBuf, tolerance: Tolerance) -> Self {
        Self {
            table,
            contract: ExitTable::vendored(),
            directory,
            tolerance,
        }
    }

    /// The directory children are run in.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Read `tool`'s version and ask its row whether this `fraise` may talk to it.
    ///
    /// This is the guard. It executes `--version` and nothing else, so a refusal costs the
    /// tool's own startup and never the verb.
    ///
    /// # Errors
    ///
    /// If the table does not name the tool, or names a version of it that is not the one
    /// installed — unless the caller asked for that skew to be tolerated.
    pub fn clear(&self, tool: &str) -> Result<Cleared<'a>, Refusal> {
        let Some(row) = self.table.tool(tool) else {
            return Err(self.refuse(unknown_tool(tool, self.table)));
        };
        let reading = tool_version::read(row.program());
        let verdict = row.judge(&reading);

        match (verdict, &reading) {
            (Verdict::Ok, _) => Ok(Cleared {
                row,
                tolerated: None,
            }),
            (Verdict::OutsideTable | Verdict::Unvouched, Reading::Version(version))
                if self.tolerance == Tolerance::TolerateSkew =>
            {
                Ok(Cleared {
                    row,
                    tolerated: Some(Skew {
                        tool: row.name().to_owned(),
                        found: version.to_string(),
                        allowed: row.allowed().map(ToOwned::to_owned),
                    }),
                })
            },
            _ => Err(self.refuse(refusal_of(row, verdict, &reading))),
        }
    }

    /// Run the verb. The only way to hold a [`Cleared`] is to have passed the guard.
    ///
    /// What was [`Asked`] for decides how the child's streams are wired. Asked for nothing,
    /// the child writes to `fraise`'s own, so a tool that streams progress to a terminal
    /// still does. Asked for an answer, its standard output is captured — an envelope and a
    /// tool cannot both own standard output — while its standard error stays the terminal's,
    /// which is where a tool's progress and diagnostics belong and where they keep arriving
    /// as they are written.
    ///
    /// # Errors
    ///
    /// If the child cannot be started — which after a successful `--version` means the machine
    /// changed underneath us.
    pub fn run<S: AsRef<OsStr>>(
        &self,
        cleared: Cleared<'a>,
        args: &[S],
        asked: Asked,
    ) -> io::Result<Outcome<'a>> {
        let mut command = Command::new(cleared.row.program());
        command.args(args).current_dir(&self.directory);
        let (status, output) = match asked {
            Asked::Nothing => (command.status()?, Vec::new()),
            Asked::Text | Asked::Json => {
                let captured = command.stdout(Stdio::piped()).spawn()?.wait_with_output()?;
                (captured.status, captured.stdout)
            },
        };
        Ok(Outcome {
            tool: cleared.row.name(),
            exit: self.mapped_exit(cleared.row.name(), status),
            tool_exit: status.code(),
            tolerated: cleared.tolerated,
            payload: Payload::captured(asked, &output),
        })
    }

    /// The tool's own exit in the umbrella's one taxonomy.
    ///
    /// A tool the exit table maps is read through its mapping; confiture is read from its own
    /// half of the contract, which is already its table. An exit the contract does not define
    /// is passed through unchanged rather than given a meaning it was not measured to have.
    fn mapped_exit(&self, tool: &str, status: ExitStatus) -> i32 {
        let Some(raw) = status.code() else {
            return signalled_exit(status);
        };
        self.contract
            .classify(tool, raw, None)
            .or_else(|| self.contract.class_of_exit(raw))
            .map_or(raw, ExitClass::exit)
    }

    const fn refuse(&self, message: String) -> Refusal {
        Refusal {
            message,
            exit: self.table.refusal_class().exit(),
        }
    }
}

/// A tool whose version has been read and allowed. Holding one is the proof the guard ran.
#[derive(Debug)]
pub struct Cleared<'a> {
    row: &'a Tool,
    tolerated: Option<Skew>,
}

impl Cleared<'_> {
    /// The skew that was let through, when the caller asked for one to be tolerated.
    #[must_use]
    pub const fn tolerated(&self) -> Option<&Skew> {
        self.tolerated.as_ref()
    }
}

/// A version the table does not allow, proceeding because the caller said to.
///
/// Its three parts are what the envelope's `tolerated` field carries; the prose below is for
/// the terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skew {
    tool: String,
    found: String,
    allowed: Option<String>,
}

impl fmt::Display for Skew {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tolerating a version skew: {} {}", self.tool, self.found)?;
        match &self.allowed {
            Some(allowed) => write!(f, ", which is outside {allowed}"),
            None => write!(f, ", which no release vouches for"),
        }
    }
}

/// The guard's refusal: what to say, and what to exit with.
#[derive(Debug)]
pub struct Refusal {
    message: String,
    exit: i32,
}

impl Refusal {
    /// What to tell the caller — the fact, then what to do about it.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The exit, which is the class the compatibility table refuses with.
    #[must_use]
    pub const fn exit(&self) -> i32 {
        self.exit
    }
}

/// What a dispatch came to, which is everything the envelope has to report about it.
#[derive(Debug)]
pub struct Outcome<'a> {
    tool: &'a str,
    exit: i32,
    tool_exit: Option<i32>,
    tolerated: Option<Skew>,
    payload: Payload,
}

impl<'a> Outcome<'a> {
    /// The tool this dispatch crossed into, as the compatibility table names it.
    #[must_use]
    pub const fn tool(&self) -> &'a str {
        self.tool
    }

    /// The exit for `fraise` itself: the tool's exit, in the umbrella's taxonomy.
    #[must_use]
    pub const fn exit(&self) -> i32 {
        self.exit
    }

    /// The tool's own exit, unmapped, or `None` when a signal ended it — a process ended by a
    /// signal never returned a number. The envelope carries this beside the mapped one; the
    /// process itself can only return one.
    #[must_use]
    pub const fn tool_exit(&self) -> Option<i32> {
        self.tool_exit
    }

    /// The skew this dispatch was told to tolerate, if any.
    #[must_use]
    pub const fn tolerated(&self) -> Option<&Skew> {
        self.tolerated.as_ref()
    }

    /// What the tool had to say, as whatever it was asked for makes it.
    #[must_use]
    pub const fn payload(&self) -> &Payload {
        &self.payload
    }
}

/// A child ended by a signal has no exit code. The contract says nothing about signals, so the
/// shell's own convention is used rather than a class that was not measured for it.
#[cfg(unix)]
fn signalled_exit(status: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt as _;

    const SHELL_SIGNAL_BASE: i32 = 128;
    SHELL_SIGNAL_BASE + status.signal().unwrap_or_default()
}

#[cfg(not(unix))]
fn signalled_exit(_status: ExitStatus) -> i32 {
    const SHELL_SIGNAL_BASE: i32 = 128;
    SHELL_SIGNAL_BASE
}

/// The umbrella speaks for the tools the table names, and says so rather than guessing.
fn unknown_tool(tool: &str, table: &CompatibilityTable) -> String {
    let known: Vec<&str> = table.tools().map(Tool::name).collect();
    format!(
        "fraise has nothing to say about {tool}: it speaks for {}.\n  \
         run `fraise doctor` to see the compatibility table.",
        known.join(", ")
    )
}

/// Why this tool cannot be dispatched to, and what would change that.
///
/// Matched on the verdict, which is the judgement; the reading only supplies the detail, and
/// where a verdict makes no promise about which reading produced it the wording says the
/// general thing rather than a pair the guard cannot reach.
fn refusal_of(row: &Tool, verdict: Verdict, reading: &Reading) -> String {
    let refuses = format!("fraise refuses to run {}", row.name());
    let install = row
        .install()
        .map_or_else(String::new, |install| format!("\n  install: {install}"));
    let hatch = "\n  tolerate: --allow-version-skew, or FRAISE_ALLOW_VERSION_SKEW=1, which \
                 reports the skew instead of refusing";
    let reasoning =
        format!("\n  run `fraise doctor` for what {} was measured against.", row.name());
    let found = match reading {
        Reading::Version(version) => version.to_string(),
        _ => "the version installed".to_owned(),
    };

    match verdict {
        Verdict::OutsideTable => format!(
            "{refuses}: {found} is outside {}.{install}{hatch}{reasoning}",
            row.allowed().unwrap_or("what it allows")
        ),
        Verdict::Unvouched => format!(
            "{refuses}: {found} is installed, and no release of it exists to vouch for \
             that.{hatch}{reasoning}"
        ),
        Verdict::Missing => format!("{refuses}: it is not on PATH.{install}{reasoning}"),
        Verdict::AwaitingRelease => format!(
            "{refuses}: it is not installed, and no release of it exists to install.{reasoning}"
        ),
        Verdict::Unreadable => {
            let problem = match reading {
                Reading::Unreadable(problem) => problem.clone(),
                _ => format!("`{} --version` named no version", row.program()),
            };
            format!(
                "{refuses}: {problem}. A version that cannot be read is not a skew, so the \
                 escape hatch does not cover it.{reasoning}"
            )
        },
        // `clear` returns `Ok` for this verdict, so reaching here would be a bug in the guard
        // rather than a state of the machine.
        Verdict::Ok => format!("{refuses}: the guard refused a version it allows"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Dispatcher, Tolerance};
    use crate::compatibility::CompatibilityTable;

    fn dispatcher(tolerance: Tolerance) -> Dispatcher<'static> {
        Dispatcher::new(CompatibilityTable::vendored(), PathBuf::from("."), tolerance)
    }

    #[test]
    fn a_tool_the_table_does_not_name_is_refused_with_the_names_it_knows() {
        let refusal = dispatcher(Tolerance::Refuse).clear("psql").expect_err("psql is not ours");
        for known in ["confiture", "fraiseql", "fraisier", "specql"] {
            assert!(refusal.message().contains(known), "{}", refusal.message());
        }
        assert_eq!(refusal.exit(), CompatibilityTable::vendored().refusal_class().exit());
    }

    /// The crate's source, one file at a time, with each file cut at its tests: test code
    /// spawns processes for its own reasons, and the claim being checked is about the code that
    /// ships.
    fn shipped_source() -> Vec<(String, String)> {
        fn collect(directory: &std::path::Path, into: &mut Vec<(String, String)>) {
            let entries = std::fs::read_dir(directory).expect("the source directory is readable");
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect(&path, into);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    let source = std::fs::read_to_string(&path).expect("a source file is read");
                    let shipped = source
                        .split_once("#[cfg(test)]")
                        .map_or_else(|| source.clone(), |(before, _)| before.to_owned());
                    let name = path
                        .strip_prefix(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
                        .expect("under src")
                        .display()
                        .to_string();
                    into.push((name, shipped));
                }
            }
        }

        let mut files = Vec::new();
        collect(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        assert!(files.len() > 4, "the scan found almost nothing, so it is measuring nothing");
        files
    }

    #[test]
    fn the_guard_is_the_only_path_to_an_exec() {
        // The invariant the whole cycle rests on, held by a test because convention is what
        // fails quietly. `unsafe_code` is forbidden in this workspace, so a child process can
        // only be started one way, and this counts the places that do it: `tool_version` reads
        // a version, `dispatch` runs a verb behind the guard, and nothing else may exec at all.
        // A new module that shells out to a tool fails here, which is the point.
        let spawn = format!("{}::{}", "Command", "new");
        let allowed = [("tool_version.rs", 1), ("dispatch.rs", 1)];

        for (name, source) in shipped_source() {
            let found = source.matches(&spawn).count();
            let expected = allowed
                .iter()
                .find(|(allowed, _)| *allowed == name)
                .map_or(0, |(_, count)| *count);
            assert_eq!(
                found, expected,
                "{name} starts {found} child processes and may start {expected}: reading a \
                 version belongs in tool_version.rs and running a verb in dispatch.rs, behind \
                 the guard"
            );
        }
    }

    #[test]
    fn an_unknown_tool_is_refused_whatever_the_caller_tolerates() {
        // The hatch is about versions that disagree; a tool the table does not name has no
        // version to disagree with. What each verdict means with a controlled `PATH` is
        // asserted in `tests/guard.rs`, since `clear` reads the real one.
        assert!(dispatcher(Tolerance::TolerateSkew).clear("psql").is_err());
    }
}
