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
//! The table names the class it refuses with, and this loader resolves that name against
//! confiture's frozen contract in [`crate::exit_table`]: one exit taxonomy for the whole
//! umbrella, and a refusal class the contract does not define cannot load.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::exit_table::{ExitClass, ExitTable};

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
    /// If the vendored table is not a document this loader accepts, or names a refusal class
    /// the exit contract does not define. It is compiled in, so that is a build the tests
    /// below would have failed before it ever shipped.
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
    /// has to say how to install one, a tool awaiting its first release must claim neither,
    /// and the refusal class has to be one `contract` defines.
    fn parse(source: &str, contract: &'static ExitTable) -> Result<Self, String> {
        let document: Document = toml::from_str(source)
            .map_err(|error| format!("this is not a compatibility table: {error}"))?;

        let refusal = contract.class_named(&document.refusal.class).ok_or_else(|| {
            format!(
                "the table refuses with {}, which the exit contract does not define, so a \
                 refusal would have no exit",
                document.refusal.class
            )
        })?;

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

        Ok(Self { tools, refusal })
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
    /// release does is a version the table cannot vouch for, and Cycle 4's guard refuses it
    /// rather than dispatching in the dark.
    #[must_use]
    pub fn accepts(&self, version: &Version) -> bool {
        self.allowed
            .as_ref()
            .is_some_and(|allowed| allowed.requirement.matches(version))
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
    refusal: Refusal,
    tools: BTreeMap<String, Row>,
}

/// The class `fraise` exits with when the table is not satisfied.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Refusal {
    class: String,
    // Reason: read by a person reading the document, not by the loader.
    #[allow(dead_code)]
    why: String,
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

    use super::{CompatibilityTable, DOCUMENT};
    use crate::exit_table::ExitTable;

    fn parse(source: &str) -> Result<CompatibilityTable, String> {
        CompatibilityTable::parse(source, ExitTable::vendored())
    }

    /// A minimal table to doctor for the refusals below: small enough that what each case
    /// changes is the only thing it says.
    const MINIMAL: &str = r#"
        [refusal]
        class = "precondition_failed"
        why = "a tool that is absent is a precondition that was not met"

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
    fn a_refusal_class_the_exit_contract_does_not_define_is_refused() {
        let source = MINIMAL.replace("precondition_failed", "catastrophe");
        let error = parse(&source).expect_err("the class is not in the contract");
        assert!(
            error.contains("catastrophe") && error.contains("exit contract"),
            "the refusal should name the class and where it should have been defined: {error}"
        );
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
