//! The compatibility table: which release of each tool this `fraise` may talk to.
//!
//! The four tools release on four schedules, so the umbrella states the range it has been
//! measured against and refuses what falls outside it. The statement is data —
//! [`compatibility.toml`](./compatibility.toml) — for the same reason the exit table is: a
//! `match` statement drifts in silence, and a document is diffed, reviewed and quoted back
//! in a report.
//!
//! One loader serves both readers of that document. `fraise doctor` walks every row and
//! reports what it measured; the version guard on each tool boundary asks one row whether
//! the version it just read is allowed. They share [`CompatibilityTable`] so there is no
//! second reading of the same file to disagree with the first.
//!
//! What it exits with when the machine does not oblige is not in this document: it is
//! [`Refusal::Compatibility`], classed in confiture's frozen contract in
//! [`crate::exit_table`]. One exit taxonomy for the whole umbrella, named in the one place
//! that already holds it.
//!
//! The judgement lives here too. [`Tool::judge`] turns what
//! [`crate::tool_version::read`] measured into a [`Verdict`], so `doctor` and the guard
//! reach the same verdict by calling the same method rather than by two matches that agree
//! today.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::exit_table::{ExitClass, ExitTable, Refusal};
use crate::tool_version::Reading;

/// The table, compiled into the binary so a released `fraise` carries the table its tests
/// and its CI job measured.
const DOCUMENT: &str = include_str!("compatibility.toml");

/// Which release of each tool this `fraise` may talk to, and what it exits with when the
/// machine does not oblige.
///
/// Obtained from [`CompatibilityTable::vendored`]; there is no other table.
#[derive(Debug)]
pub struct CompatibilityTable {
    tools: BTreeMap<String, Tool>,
    refusal: ExitClass<'static>,
}

impl CompatibilityTable {
    /// The vendored table, parsed and checked once per process.
    ///
    /// # Panics
    ///
    /// If the vendored table is not a document this loader accepts. It is compiled in, so that
    /// is a build the tests below would have failed before it ever shipped.
    #[must_use]
    pub fn vendored() -> &'static Self {
        static TABLE: OnceLock<CompatibilityTable> = OnceLock::new();
        TABLE.get_or_init(|| {
            Self::parse(DOCUMENT, ExitTable::vendored())
                .unwrap_or_else(|error| panic!("the compatibility table: {error}"))
        })
    }

    /// Every tool the table speaks for, in name order.
    #[must_use]
    pub fn tools(&self) -> impl ExactSizeIterator<Item = &Tool> {
        self.tools.values()
    }

    /// The row for one tool, or `None` when the table speaks for no such tool — which is a
    /// refusal to dispatch rather than a permission.
    #[must_use]
    pub fn tool(&self, name: &str) -> Option<&Tool> {
        self.tools.get(name)
    }

    /// The class `fraise` exits with when this table is not satisfied, as the table names it
    /// and the exit contract defines it.
    #[must_use]
    pub const fn refusal_class(&self) -> ExitClass<'static> {
        self.refusal
    }

    /// Parse a table and refuse one that cannot be acted on.
    ///
    /// Checked at load rather than at the point of use, so a row that could vouch for a
    /// version nobody measured cannot exist: a range has to parse, a tool that has releases
    /// has to say how to install one, and a tool awaiting its first release must claim
    /// neither. `contract` is what the refusal class comes from.
    fn parse(source: &str, contract: &'static ExitTable) -> Result<Self, String> {
        let document: Document = toml::from_str(source)
            .map_err(|error| format!("this is not a compatibility table: {error}"))?;

        let mut tools = BTreeMap::new();
        for (name, row) in document.tools {
            let allowed = match (row.awaiting_release, row.versions, row.install) {
                (true, None, None) => None,
                (true, ..) => {
                    return Err(format!(
                        "{name} is marked as awaiting a release and still names a version range \
                         or a way to install one"
                    ));
                },
                (false, Some(versions), Some(install)) => {
                    let requirement = VersionReq::parse(&versions).map_err(|error| {
                        format!("{name}'s allowed versions {versions:?} are not a range: {error}")
                    })?;
                    Some(Allowed {
                        requirement,
                        written: versions,
                        install,
                    })
                },
                (false, ..) => {
                    return Err(format!(
                        "{name} must either allow a range of versions and say how to install one, \
                         or be marked `awaiting_release = true`"
                    ));
                },
            };
            tools.insert(
                name.clone(),
                Tool {
                    name,
                    program: row.program,
                    allowed,
                    why: row.why,
                },
            );
        }

        Ok(Self {
            tools,
            refusal: contract.refusal(Refusal::Compatibility),
        })
    }
}

/// One tool's row: the program to run, the versions of it this `fraise` was measured
/// against, and why that is the range.
#[derive(Debug)]
pub struct Tool {
    name: String,
    program: String,
    allowed: Option<Allowed>,
    why: String,
}

impl Tool {
    /// The tool's name in the table, which is the name a report and an envelope use.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The program to execute, looked up on `PATH`.
    #[must_use]
    pub fn program(&self) -> &str {
        &self.program
    }

    /// The allowed range as the table writes it, or `None` when no release exists to
    /// require. Reported rather than matched on: a reader who is told what was allowed can
    /// act without opening this repository.
    #[must_use]
    pub fn allowed(&self) -> Option<&str> {
        self.allowed.as_ref().map(|allowed| allowed.written.as_str())
    }

    /// The command that installs an allowed release, or `None` when there is none to
    /// install. CI runs these, so they are measured rather than remembered.
    #[must_use]
    pub fn install(&self) -> Option<&str> {
        self.allowed.as_ref().map(|allowed| allowed.install.as_str())
    }

    /// Why the range is what it is — carried into every report, so a refusal explains
    /// itself where it is read.
    #[must_use]
    pub fn why(&self) -> &str {
        &self.why
    }

    /// Whether the table has a release of this tool to require at all.
    #[must_use]
    pub const fn pins_a_release(&self) -> bool {
        self.allowed.is_some()
    }

    /// Whether `version` is one this `fraise` may talk to.
    ///
    /// A tool awaiting its first release accepts nothing: a build that exists while no
    /// release does is a version the table cannot vouch for, and the version guard refuses it
    /// rather than dispatching in the dark.
    #[must_use]
    pub fn accepts(&self, version: &Version) -> bool {
        self.allowed
            .as_ref()
            .is_some_and(|allowed| allowed.requirement.matches(version))
    }

    /// What this row says about what was measured on the machine.
    ///
    /// The one place a reading becomes a verdict. `doctor` prints the verdict and the guard
    /// refuses on it, so neither holds a rule of its own about what an absent tool or an
    /// unreadable version means.
    #[must_use]
    pub fn judge(&self, reading: &Reading) -> Verdict {
        match (self.pins_a_release(), reading) {
            (true, Reading::Version(version)) => {
                if self.accepts(version) {
                    Verdict::Ok
                } else {
                    Verdict::OutsideTable
                }
            },
            (false, Reading::Version(_)) => Verdict::Unvouched,
            (true, Reading::Missing) => Verdict::Missing,
            (false, Reading::Missing) => Verdict::AwaitingRelease,
            (_, Reading::Unreadable(_)) => Verdict::Unreadable,
        }
    }
}

/// What the table says about one tool as it is installed here.
///
/// Two of these satisfy the table and four do not, which [`Verdict::satisfied`] answers. The
/// four are kept apart because a reader acts differently on each — install it, downgrade it,
/// look at what it printed, or stop dispatching to a build nothing released — and collapsing
/// them into "not ok" is the report failing at its one job.
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
    /// for that version, so the guard refuses to dispatch to it.
    Unvouched,
}

impl Verdict {
    /// Whether this verdict satisfies the table.
    #[must_use]
    pub const fn satisfied(self) -> bool {
        matches!(self, Self::Ok | Self::AwaitingRelease)
    }
}

/// The versions of a tool that exist and are allowed, with the command that gets one.
#[derive(Debug)]
struct Allowed {
    requirement: VersionReq,
    written: String,
    install: String,
}

/// The table as it is written. Every section denies unknown fields — this document is ours,
/// so a misspelled key is a mistake rather than news from upstream — and `why` is required
/// everywhere, which is how no range can be added without saying what it was measured
/// against.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    tools: BTreeMap<String, Row>,
}

/// One row as it is written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    program: String,
    /// Set when no release of the tool exists yet, in which case the row claims no range and
    /// no install command; [`CompatibilityTable::parse`] refuses a row that does both.
    #[serde(default)]
    awaiting_release: bool,
    versions: Option<String>,
    install: Option<String>,
    why: String,
}

#[cfg(test)]
mod tests {
    use semver::Version;

    use super::{CompatibilityTable, DOCUMENT, Verdict};
    use crate::exit_table::{ExitTable, Refusal};
    use crate::tool_version::Reading;

    fn parse(source: &str) -> Result<CompatibilityTable, String> {
        CompatibilityTable::parse(source, ExitTable::vendored())
    }

    /// A minimal table to doctor for the refusals below: small enough that what each case
    /// changes is the only thing it says.
    const MINIMAL: &str = r#"
        [tools.confiture]
        program = "confiture"
        versions = ">=1.19.0, <1.20.0"
        why = "the release the vendored exit table was taken from"
        install = "uv tool install --force fraiseql-confiture==1.19.0"
    "#;

    #[test]
    fn the_vendored_table_speaks_for_the_four_tools_of_the_stack() {
        let table = CompatibilityTable::vendored();
        let named: Vec<&str> = table.tools().map(super::Tool::name).collect();
        assert_eq!(named, ["confiture", "fraiseql", "fraisier", "specql"]);
        assert_eq!(table.tools().len(), 4);
        assert!(table.tool("psql").is_none(), "the table speaks only for what it names");
    }

    #[test]
    fn the_confiture_row_accepts_the_release_the_exit_table_is_pinned_to() {
        // The pin has one home, `tools/confiture-requirements.txt`, and the exit table's
        // freshness test measures the contract against exactly that release. A range here
        // that did not accept it would let `doctor` bless a confiture the contract was never
        // taken from, so the two are tied together by this test rather than by a comment.
        let pin = pinned_confiture_version();
        let version = Version::parse(&pin).expect("the pinned confiture is a semver version");
        let confiture = CompatibilityTable::vendored()
            .tool("confiture")
            .expect("the table names confiture");
        assert!(
            confiture.accepts(&version),
            "the table allows {} but the pinned confiture is {pin}",
            confiture.allowed().unwrap_or("nothing")
        );
        assert!(
            confiture.install().is_some_and(|install| install.contains(&pin)),
            "the install command should name the pinned release: {:?}",
            confiture.install()
        );
    }

    /// The pinned confiture, read where the exit table's own test reads it.
    fn pinned_confiture_version() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("the workspace root is two levels above crates/<name>")
            .join("tools/confiture-requirements.txt");
        let pins = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("reading the confiture pin {}: {error}", path.display())
        });
        pins.lines()
            .find_map(|line| line.trim().strip_prefix("fraiseql-confiture=="))
            .unwrap_or_else(|| {
                panic!("{} names no `fraiseql-confiture==<version>`", path.display())
            })
            .to_owned()
    }

    #[test]
    fn a_development_build_is_outside_the_table_its_release_would_be_inside() {
        // The property the whole table leans on, and the one a reader is most likely to
        // doubt: `2.14.2-dev.<sha>` sits between the range's bounds numerically, and cargo's
        // semver still rejects it, because no comparator names a prerelease. A build nobody
        // released is a version nobody measured.
        let fraiseql = CompatibilityTable::vendored()
            .tool("fraiseql")
            .expect("the table names fraiseql");
        assert!(fraiseql.accepts(&Version::parse("2.14.1").expect("a version")));
        assert!(!fraiseql.accepts(&Version::parse("2.14.2-dev.768dff0b8").expect("a version")));
        assert!(!fraiseql.accepts(&Version::parse("2.15.0").expect("a version")));
    }

    #[test]
    fn a_tool_awaiting_its_first_release_accepts_no_version_at_all() {
        let specql = CompatibilityTable::vendored().tool("specql").expect("the table names specql");
        assert!(!specql.pins_a_release());
        assert_eq!(specql.allowed(), None);
        assert_eq!(specql.install(), None);
        assert!(!specql.accepts(&Version::parse("2.0.0").expect("a version")));
    }

    #[test]
    fn a_reading_becomes_a_verdict_in_one_place_for_every_caller() {
        // `doctor` reports these and the guard refuses on them, so the four unsatisfied
        // states are asserted here rather than in either caller.
        let table = CompatibilityTable::vendored();
        let fraiseql = table.tool("fraiseql").expect("the table names fraiseql");
        let specql = table.tool("specql").expect("the table names specql");
        let version = |text: &str| Reading::Version(Version::parse(text).expect("a version"));

        assert_eq!(fraiseql.judge(&version("2.14.1")), Verdict::Ok);
        assert_eq!(fraiseql.judge(&version("2.13.0")), Verdict::OutsideTable);
        assert_eq!(fraiseql.judge(&Reading::Missing), Verdict::Missing);
        assert_eq!(
            fraiseql.judge(&Reading::Unreadable("said nothing".to_owned())),
            Verdict::Unreadable
        );
        assert_eq!(specql.judge(&Reading::Missing), Verdict::AwaitingRelease);
        assert_eq!(specql.judge(&version("2.0.0")), Verdict::Unvouched);

        let unsatisfied = [
            Verdict::OutsideTable,
            Verdict::Missing,
            Verdict::Unreadable,
            Verdict::Unvouched,
        ];
        assert!(!unsatisfied.iter().any(|verdict| verdict.satisfied()));
        assert!(Verdict::Ok.satisfied() && Verdict::AwaitingRelease.satisfied());
    }

    #[test]
    fn the_class_this_table_refuses_with_comes_from_the_contract() {
        // Not from this document: it is the face's own `compatibility_unsatisfied` refusal, and
        // the contract is what gives it an exit. `doctor` prints that exit and the guard exits
        // with it, so a table that answered for it here would be a second taxonomy.
        let refusal = CompatibilityTable::vendored().refusal_class();
        assert_eq!(refusal, ExitTable::vendored().refusal(Refusal::Compatibility));
        assert_ne!(refusal.exit(), 0, "a refusal is not a success: {refusal:?}");
    }

    #[test]
    fn a_row_that_both_awaits_a_release_and_names_one_is_refused() {
        let source = MINIMAL.replace(
            r#"program = "confiture""#,
            "program = \"confiture\"\nawaiting_release = true",
        );
        let error = parse(&source).expect_err("the row says two things at once");
        assert!(
            error.contains("confiture") && error.contains("awaiting a release"),
            "the refusal should name the tool and the contradiction: {error}"
        );
    }

    #[test]
    fn a_row_that_allows_versions_without_saying_how_to_install_one_is_refused() {
        // A report that names a range and cannot say how to satisfy it sends its reader
        // looking through this repository, which is the failure this field prevents.
        let source = MINIMAL
            .lines()
            .filter(|line| !line.trim_start().starts_with("install ="))
            .collect::<Vec<_>>()
            .join("\n");
        let error = parse(&source).expect_err("the row cannot be acted on");
        assert!(
            error.contains("how to install"),
            "the refusal should say what is missing: {error}"
        );
    }

    #[test]
    fn a_range_that_is_not_a_range_is_refused() {
        let source = MINIMAL.replace(">=1.19.0, <1.20.0", "the newest one");
        let error = parse(&source).expect_err("that is not a version requirement");
        assert!(
            error.contains("the newest one"),
            "the refusal should quote what could not be read as a range: {error}"
        );
    }

    #[test]
    fn a_misspelled_field_is_refused_rather_than_ignored() {
        // This half of the umbrella's documents is ours, so an unknown key is a mistake. A
        // tolerated one would be a range or a reason that silently does nothing.
        let source = MINIMAL.replace("versions =", "version =");
        let error = parse(&source).expect_err("`version` is not a field of a row");
        assert!(error.contains("version"), "the refusal should name the key: {error}");
    }

    #[test]
    fn the_vendored_table_is_the_one_the_binary_carries() {
        // `DOCUMENT` is what `vendored()` parses; parsing it again here is what makes the
        // refusal tests above statements about the real loader rather than about a fixture.
        assert!(parse(DOCUMENT).is_ok(), "the vendored table loads");
    }
}
