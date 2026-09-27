//! Confiture's exit-code contract, vendored whole, with the face's mapping inside it.
//!
//! Three of the four tools under `fraise` number their failures their own way, and none
//! of those numbers is a contract. Confiture's is: nine semantic classes over exits
//! `0..=8`, frozen since its issue #146, emitted as one JSON document by
//! `confiture --exit-codes-json`. The face adopts that taxonomy whole rather than
//! inventing a tenth, so [`exit_table.vendored.json`](./exit_table.vendored.json) is that
//! document captured from the pinned release — **plus one section of our own**,
//! `mappings`, saying what each tool's raw exit means in confiture's classes.
//!
//! The mapping lives *inside* the vendored document on purpose. A `match` statement
//! drifts in silence; a document is diffed.
//!
//! No class name is ever written in this crate's source, and the types are what stop it:
//! an [`ExitClass`] is built only by [`ExitTable`] out of the document, from a name the
//! document defines, so there is no literal a caller could compare against and no way to
//! spell a tenth class. [`ExitTable::parse`] refuses a document whose halves disagree —
//! a mapping row onto a class the contract does not define, a class given to two exits —
//! so the guard is in force at load rather than discovered on a live dispatch.
//!
//! The face's own refusals are in that section too, as [`Refusal`]: a machine that does not
//! satisfy the compatibility table and a `fraise.toml` nothing can act on are failures of
//! `fraise` rather than of a tool, and saying what they come to is the same kind of statement as
//! saying what fraiseql's exit 2 comes to. They are here rather than in the documents that
//! discover them, so that one document in the tree names a class and the source names only
//! refusals — a second place naming classes is a second taxonomy waiting to disagree.
//!
//! The freshness test below compares the confiture half **whole** against what the
//! pinned confiture emits, and **fails rather than skips** when confiture is missing or
//! is a different release. The pin is `tools/confiture-requirements.txt`, CI installs it
//! before the gate, and adopting a confiture change is one commit that bumps the pin and
//! regenerates the document together — either half alone fails the test:
//!
//! ```sh
//! uv venv --python 3.11 /tmp/confiture
//! uv pip install --python /tmp/confiture/bin/python -r tools/confiture-requirements.txt
//! /tmp/confiture/bin/confiture --exit-codes-json >/tmp/exit-codes.json
//! python3 - <<'PY'
//! import json, pathlib
//! path = pathlib.Path("crates/fraise/src/exit_table.vendored.json")
//! document = json.loads(pathlib.Path("/tmp/exit-codes.json").read_text())
//! document["mappings"] = json.loads(path.read_text())["mappings"]
//! path.write_text(json.dumps(document, indent=2) + "\n")
//! PY
//! ```
//!
//! The merge keeps the face's section and takes everything else from the tool;
//! `json.dumps(…, indent=2)` is confiture's own rendering, so the confiture half of the
//! file comes back byte for byte.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::Deserialize;

/// The contract document, compiled into the binary so a released `fraise` carries the
/// same table its tests measured.
const VENDORED: &str = include_str!("exit_table.vendored.json");

/// One class of confiture's taxonomy, as the vendored document defines it.
///
/// There is no constructor: a class is handed out by [`ExitTable`] and borrows its name
/// from the document, which is what keeps the taxonomy in the document and out of the
/// source. Two classes are equal when they are the same class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitClass<'a> {
    name: &'a str,
    exit: i32,
    meaning: &'a str,
}

impl<'a> ExitClass<'a> {
    /// The class's name, as confiture spells it on the wire.
    #[must_use]
    pub const fn name(self) -> &'a str {
        self.name
    }

    /// The exit integer confiture itself returns for this class. Classes and exits are
    /// one-to-one, which [`ExitTable::parse`] enforces.
    #[must_use]
    pub const fn exit(self) -> i32 {
        self.exit
    }

    /// Confiture's one-line meaning for that exit — the sentence to show a person, rather
    /// than the name to match on.
    #[must_use]
    pub const fn meaning(self) -> &'a str {
        self.meaning
    }
}

/// One refusal `fraise` makes on its own account, which the contract gives a class.
///
/// A refusal is named here and classed in the document, never the other way round: the
/// variants are what the binary can refuse for, and [`ExitTable::parse`] holds the document to
/// naming every one of them, so a refusal that could not be given an exit fails at load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// This machine does not satisfy the compatibility table: a tool is absent, or is at a
    /// version nothing measured. `doctor` reports it and the version guard refuses on it.
    Compatibility,
    /// `fraise.toml` is not a document this face can act on.
    Configuration,
}

impl Refusal {
    /// Every refusal the binary can make, which is what the document must speak for.
    const ALL: [Self; 2] = [Self::Compatibility, Self::Configuration];

    /// The key this refusal has in the contract's `mappings.refusals` section.
    const fn key(self) -> &'static str {
        match self {
            Self::Compatibility => "compatibility_unsatisfied",
            Self::Configuration => "invalid_configuration",
        }
    }
}

/// Confiture's exit-code contract, with the per-tool mapping that turns another tool's
/// raw exit into one of its classes, and the classes the face's own refusals read as.
///
/// Obtained from [`ExitTable::vendored`]; there is no other table.
#[derive(Debug)]
pub struct ExitTable {
    exits: BTreeMap<i32, ExitEntry>,
    refusals: BTreeMap<String, RefusalEntry>,
    tools: BTreeMap<String, ToolMapping>,
}

impl ExitTable {
    /// The vendored table, parsed and checked once per process.
    ///
    /// # Panics
    ///
    /// If the vendored document is not a contract this loader accepts. It is compiled in,
    /// so that is a build the tests below would have failed before it ever shipped.
    #[must_use]
    pub fn vendored() -> &'static Self {
        static TABLE: OnceLock<ExitTable> = OnceLock::new();
        TABLE.get_or_init(|| {
            Self::parse(VENDORED).unwrap_or_else(|error| panic!("the vendored contract: {error}"))
        })
    }

    /// The semantic class confiture gives its own exit `exit`, or `None` for an exit it
    /// does not document.
    ///
    /// This is how a *confiture* exit is read. Confiture has no entry under `mappings`
    /// because this map is already its table, and one table is the point.
    #[must_use]
    pub fn class_of_exit(&self, exit: i32) -> Option<ExitClass<'_>> {
        let entry = self.exits.get(&exit)?;
        Some(ExitClass {
            name: &entry.class,
            exit,
            meaning: &entry.meaning,
        })
    }

    /// The confiture class that `tool`'s exit `tool_exit` means.
    ///
    /// `error_class` is the tool's own error taxonomy name when the tool reports one, and
    /// `None` when it does not — which is every tool today. A row for the exact pair wins;
    /// otherwise the exit's unrefined row answers; otherwise the tool's `unlisted` class
    /// does. `None` means the table knows no such tool, which is not a classification but
    /// a refusal.
    #[must_use]
    pub fn classify(
        &self,
        tool: &str,
        tool_exit: i32,
        error_class: Option<&str>,
    ) -> Option<ExitClass<'_>> {
        let mapping = self.tools.get(tool)?;
        let refined = error_class.and_then(|class| {
            mapping
                .rows
                .iter()
                .find(|row| row.tool_exit == tool_exit && row.error_class.as_deref() == Some(class))
        });
        let row = refined.or_else(|| {
            mapping
                .rows
                .iter()
                .find(|row| row.tool_exit == tool_exit && row.error_class.is_none())
        });
        let name = row.map_or(mapping.unlisted.class.as_str(), |row| row.class.as_str());
        self.class_named(name)
    }

    /// The class `fraise` exits with when it refuses for `which` reason.
    ///
    /// Infallible, because [`ExitTable::parse`] has already held the document to giving every
    /// refusal a class the contract defines — the check is at load so that no refusal can be
    /// discovered to have no exit at the moment it is being made.
    ///
    /// # Panics
    ///
    /// If it is reached on a table that was not parsed, which no code path allows.
    #[must_use]
    pub fn refusal(&self, which: Refusal) -> ExitClass<'_> {
        let entry = self
            .refusals
            .get(which.key())
            .expect("parse requires the document to name every refusal");
        self.class_named(&entry.class)
            .expect("parse requires every refusal's class to be one the contract defines")
    }

    /// The class of that name, or `None` when the contract defines none.
    ///
    /// Private: a name is how this document refers to a class, and nothing outside gets to
    /// hold one. Every caller reaches a class through what it is asking about — a confiture
    /// exit, a tool's exit, or one of the face's own refusals — so there is no path by which a
    /// class name could be written in the source and resolved here.
    fn class_named(&self, name: &str) -> Option<ExitClass<'_>> {
        let (exit, entry) = self.exits.iter().find(|(_, entry)| entry.class == name)?;
        Some(ExitClass {
            name: &entry.class,
            exit: *exit,
            meaning: &entry.meaning,
        })
    }

    /// Parse a contract document and refuse one whose halves disagree.
    ///
    /// Checked here rather than at the point of use, so a document that could misclassify
    /// a dispatch cannot load at all: every class an exit, a mapping row or a refusal names
    /// is one the contract lists, no class is given to two exits (or reading a class back
    /// would be ambiguous), every class has an exit, no tool maps the same
    /// `(exit, error class)` twice, and the refusals section speaks for exactly the refusals
    /// the binary can make — one it does not know is as wrong as one it cannot find.
    fn parse(source: &str) -> Result<Self, String> {
        let document: Document = serde_json::from_str(source)
            .map_err(|error| format!("this is not a contract document: {error}"))?;
        let known = |class: &str, named_by: &str| {
            if document.classes.iter().any(|listed| listed == class) {
                Ok(())
            } else {
                Err(format!(
                    "{named_by} is classed {class}, which the contract's `classes` do not list"
                ))
            }
        };

        let mut exits = BTreeMap::new();
        for (exit, entry) in document.exit_codes {
            let exit: i32 = exit
                .parse()
                .map_err(|_| format!("`exit_codes` is keyed on {exit:?}, not an exit integer"))?;
            known(&entry.class, &format!("exit {exit}"))?;
            if exits.values().any(|other: &ExitEntry| other.class == entry.class) {
                return Err(format!(
                    "the class {} is given to more than one exit, so reading it back would be \
                     ambiguous",
                    entry.class
                ));
            }
            exits.insert(exit, entry);
        }
        if exits.len() != document.classes.len() {
            return Err(format!(
                "the contract names {} classes and gives exits to {}, so at least one class \
                 cannot be reached",
                document.classes.len(),
                exits.len()
            ));
        }

        for (named, refusal) in &document.mappings.refusals.reasons {
            known(&refusal.class, &format!("the {named} refusal"))?;
            if !Refusal::ALL.iter().any(|which| which.key() == named) {
                return Err(format!(
                    "the document classes a {named} refusal, which this fraise does not make, so \
                     nothing would ever read it"
                ));
            }
        }
        for which in Refusal::ALL {
            if !document.mappings.refusals.reasons.contains_key(which.key()) {
                return Err(format!(
                    "the document gives no class to the {} refusal, which this fraise makes, so \
                     that refusal would have no exit",
                    which.key()
                ));
            }
        }

        for (tool, mapping) in &document.mappings.tools {
            let mut seen = BTreeSet::new();
            for row in &mapping.rows {
                known(&row.class, &format!("{tool}'s exit {}", row.tool_exit))?;
                if !seen.insert((row.tool_exit, row.error_class.clone())) {
                    return Err(format!(
                        "{tool} maps exit {} / error class {:?} more than once",
                        row.tool_exit, row.error_class
                    ));
                }
            }
            known(&mapping.unlisted.class, &format!("{tool}'s unlisted exits"))?;
        }

        Ok(Self {
            exits,
            refusals: document.mappings.refusals.reasons,
            tools: document.mappings.tools,
        })
    }
}

/// The document as it is written. Confiture's half tolerates fields this loader does not
/// read — the freshness test is what holds that half to the tool, and a released binary
/// should not refuse to start because confiture added a key. The face's half does not:
/// see [`Mappings`].
#[derive(Debug, Deserialize)]
struct Document {
    classes: Vec<String>,
    exit_codes: BTreeMap<String, ExitEntry>,
    mappings: Mappings,
}

/// One exit of confiture's own table.
#[derive(Debug, Deserialize)]
struct ExitEntry {
    class: String,
    meaning: String,
}

/// The face's section. It and everything under it deny unknown fields: this half is ours,
/// so a misspelled key is a mistake rather than news from upstream — and `note` and `why`
/// being required is how a row cannot be added without saying why it reads that way.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mappings {
    // Reason: read by a person reading the document, not by the loader.
    #[allow(dead_code)]
    note: String,
    refusals: Refusals,
    tools: BTreeMap<String, ToolMapping>,
}

/// The section that says what the face's own refusals come to.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Refusals {
    // Reason: read by a person reading the document, not by the loader.
    #[allow(dead_code)]
    note: String,
    reasons: BTreeMap<String, RefusalEntry>,
}

/// What one of the face's own refusals reads as. Keyed by [`Refusal::key`], so the document
/// and the binary name the same refusals or neither loads.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RefusalEntry {
    class: String,
    // Reason: why that class rather than a neighbouring one; required, so it is given.
    #[allow(dead_code)]
    why: String,
}

/// One tool's mapping into confiture's classes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolMapping {
    // Reason: where the tool's own exits were read, and when.
    #[allow(dead_code)]
    source: String,
    // Reason: what was measured, including what is deliberately unmapped.
    #[allow(dead_code)]
    note: String,
    rows: Vec<Row>,
    unlisted: Unlisted,
}

/// One row: this exit, under this error class, reads as this confiture class.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    tool_exit: i32,
    /// The tool's own error taxonomy name, or `null` for the unrefined reading of that
    /// exit. No tool reports one at the process boundary yet.
    error_class: Option<String>,
    class: String,
    // Reason: the reason the row reads that way; required, so it is given.
    #[allow(dead_code)]
    why: String,
}

/// What an exit the tool does not document reads as.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Unlisted {
    class: String,
    // Reason: the reason the fallback is that class; required.
    #[allow(dead_code)]
    why: String,
}

#[cfg(test)]
mod tests {
    use super::{ExitTable, Refusal, VENDORED};
    use crate::pinned;

    /// D7's mapping, written in the decision's own vocabulary — a tool's raw exit, the
    /// error class it reports when it reports one, and the **confiture exit integer** the
    /// face reads it as. Integers on the right so this table is an independent statement of
    /// the decision ("fraiseql 2 → 5, specql 1 → 4/5 by class, fraisier 1 → 1") rather than
    /// a second reading of the document under test, and so it names no class either.
    const MAPPING: &[(&str, i32, Option<&str>, i32)] = &[
        // fraiseql: 0 success, 1 error, 2 validation_failed — its whole documented table
        // (`get_exit_codes()`, enforced by `enforce_exit_code()`).
        ("fraiseql", 0, None, 0),
        ("fraiseql", 1, None, 1),
        ("fraiseql", 2, None, 5),
        // An exit no tool documents falls to that tool's `unlisted` class.
        ("fraiseql", 9, None, 1),
        // specql: every failure is exit 1, refined by the error class it will report.
        ("specql", 0, None, 0),
        ("specql", 1, None, 5),
        ("specql", 1, Some("TypeError"), 4),
        ("specql", 1, Some("ConstraintError"), 4),
        ("specql", 1, Some("ReferenceError"), 4),
        ("specql", 1, Some("Parse"), 5),
        ("specql", 1, Some("TomlParseError"), 5),
        ("specql", 1, Some("ValidationError"), 5),
        // A class the tool does not list falls back to the exit's unrefined row.
        ("specql", 1, Some("NoSuchVariant"), 5),
        // fraisier: 0, and 1 for a failed command.
        ("fraisier", 0, None, 0),
        ("fraisier", 1, None, 1),
    ];

    #[test]
    fn the_mapping_rows_come_from_the_vendored_document() {
        let table = ExitTable::vendored();
        for (tool, tool_exit, error_class, confiture_exit) in MAPPING {
            let class = table.classify(tool, *tool_exit, *error_class).unwrap_or_else(|| {
                panic!("the table maps no exit of {tool}: classify({tool_exit}, {error_class:?})")
            });
            assert_eq!(
                class.exit(),
                *confiture_exit,
                "{tool} exit {tool_exit} (error class {error_class:?}) reads as {} ({}), and the \
                 mapping says it should read as confiture's exit {confiture_exit}",
                class.name(),
                class.meaning()
            );
        }
    }

    #[test]
    fn a_tool_the_table_does_not_name_is_not_classified() {
        // The guard refuses to dispatch to a tool the table has nothing to say about, so an
        // unknown tool must be `None` rather than some benign default.
        assert_eq!(ExitTable::vendored().classify("psql", 1, None), None);
    }

    #[test]
    fn a_confiture_exit_is_read_from_the_contract_itself() {
        // Confiture is absent from `mappings` on purpose: its own half of the document is
        // already its table, so the loader must answer for it without a mapping row.
        let table = ExitTable::vendored();
        let ok = table.class_of_exit(0).expect("confiture documents exit 0");
        assert_eq!(ok.exit(), 0);
        assert_eq!(table.classify("confiture", 0, None), None);
        assert_eq!(table.class_of_exit(9), None, "confiture documents 0..=8");
    }

    /// The vendored document with one thing changed, for the refusals below: the guard
    /// matters only if it is in force, and the way to show that is to hand it a document it
    /// must refuse.
    fn doctored(change: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut document: serde_json::Value =
            serde_json::from_str(VENDORED).expect("the vendored document parses");
        change(&mut document);
        document.to_string()
    }

    #[test]
    fn a_mapping_onto_a_class_the_contract_does_not_define_is_refused() {
        let source = doctored(|document| {
            document["mappings"]["tools"]["specql"]["rows"][0]["class"] =
                serde_json::Value::from("catastrophe");
        });
        let error = ExitTable::parse(&source).expect_err("the halves disagree");
        assert!(
            error.contains("catastrophe") && error.contains("classes"),
            "the refusal should name the class and where it should have been listed: {error}"
        );
    }

    #[test]
    fn a_class_given_to_two_exits_is_refused() {
        // `classify` reaches a class by name, so a name that belongs to two exits would make
        // the exit it reports depend on iteration order.
        let source = doctored(|document| {
            let ok = document["exit_codes"]["0"]["class"].clone();
            document["exit_codes"]["4"]["class"] = ok;
        });
        let error = ExitTable::parse(&source).expect_err("a class belongs to one exit");
        assert!(
            error.contains("more than one exit"),
            "the refusal should say the class is not one-to-one: {error}"
        );
    }

    #[test]
    fn a_refusal_classed_as_something_the_contract_does_not_define_is_refused() {
        let source = doctored(|document| {
            document["mappings"]["refusals"]["reasons"]["invalid_configuration"]["class"] =
                serde_json::Value::from("catastrophe");
        });
        let error = ExitTable::parse(&source).expect_err("the class is not in the contract");
        assert!(
            error.contains("catastrophe") && error.contains("classes"),
            "the refusal should name the class and where it should have been listed: {error}"
        );
    }

    #[test]
    fn a_refusal_the_binary_makes_and_the_document_does_not_class_is_refused() {
        // The failure this prevents is a refusal with no exit, discovered at the moment it is
        // being made — which is the moment a caller has the least to go on.
        let source = doctored(|document| {
            document["mappings"]["refusals"]["reasons"]
                .as_object_mut()
                .expect("the reasons are an object")
                .remove("invalid_configuration");
        });
        let error = ExitTable::parse(&source).expect_err("a refusal has no class");
        assert!(
            error.contains("invalid_configuration"),
            "the refusal should name the one that would have no exit: {error}"
        );
    }

    #[test]
    fn a_refusal_the_binary_does_not_make_is_refused_rather_than_ignored() {
        // The other direction, and the reason the keys are the binary's: a classed refusal
        // nothing reads is a decision recorded where a reader would take it for one in force.
        let source = doctored(|document| {
            // Classed as something the contract does define, so what is refused is the key and
            // not the class — and taken from the document, so this test names no class either.
            let classed =
                document["mappings"]["refusals"]["reasons"]["invalid_configuration"]["class"]
                    .clone();
            document["mappings"]["refusals"]["reasons"]["spilled_the_jam"] = serde_json::json!({
                "class": classed,
                "why": "a refusal this fraise has never heard of",
            });
        });
        let error = ExitTable::parse(&source).expect_err("the binary makes no such refusal");
        assert!(
            error.contains("spilled_the_jam"),
            "the refusal should name the key nothing reads: {error}"
        );
    }

    #[test]
    fn every_refusal_the_binary_makes_reads_as_a_class_of_the_contract() {
        // What the accessor promises, said for each variant rather than for the one a test
        // happened to reach: `refusal` is infallible because `parse` has already been through
        // this list.
        let table = ExitTable::vendored();
        for which in Refusal::ALL {
            let class = table.refusal(which);
            assert_eq!(
                table.class_of_exit(class.exit()),
                Some(class),
                "{which:?} reads as {} which is not the class of exit {}",
                class.name(),
                class.exit()
            );
            assert_ne!(class.exit(), 0, "{which:?} is a refusal, not a success");
        }
    }

    #[test]
    fn a_row_without_its_reason_is_refused() {
        // `why` is required so that no exit acquires a reading nobody had to justify.
        let source = doctored(|document| {
            let row = &mut document["mappings"]["tools"]["fraisier"]["rows"][1];
            row.as_object_mut().expect("a row is an object").remove("why");
        });
        let error = ExitTable::parse(&source).expect_err("the row gives no reason");
        assert!(error.contains("why"), "the refusal should name the missing field: {error}");
    }

    /// The paths at which two `--exit-codes-json` documents differ, one per line, so the
    /// failure below names what drifted instead of printing two documents and leaving the
    /// reader to find it. Only a hint: the assertion compares the documents whole, so a
    /// difference this walker cannot localise still fails.
    fn describe_drift(live: &serde_json::Value, vendored: &serde_json::Value) -> String {
        /// The union of two objects' field names, so a field missing on one side is
        /// reported rather than skipped.
        fn names(
            left: Option<&serde_json::Value>,
            right: Option<&serde_json::Value>,
        ) -> std::collections::BTreeSet<String> {
            [left, right]
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_object)
                .flat_map(|object| object.keys().cloned())
                .collect()
        }
        fn show(value: Option<&serde_json::Value>) -> String {
            value.map_or_else(|| "absent".to_owned(), ToString::to_string)
        }

        let mut lines = Vec::new();
        for key in names(Some(live), Some(vendored)) {
            let (live_value, vendored_value) = (live.get(&key), vendored.get(&key));
            if live_value == vendored_value {
                continue;
            }
            if key != "exit_codes" {
                lines.push(format!(
                    "  {key}: vendored {}, confiture {}",
                    show(vendored_value),
                    show(live_value)
                ));
                continue;
            }
            for exit in names(live_value, vendored_value) {
                let live_exit = live_value.and_then(|table| table.get(&exit));
                let vendored_exit = vendored_value.and_then(|table| table.get(&exit));
                if live_exit == vendored_exit {
                    continue;
                }
                for field in names(live_exit, vendored_exit) {
                    let live_field = live_exit.and_then(|entry| entry.get(&field));
                    let vendored_field = vendored_exit.and_then(|entry| entry.get(&field));
                    if live_field != vendored_field {
                        lines.push(format!(
                            "  exit {exit} {field}: vendored {}, confiture {}",
                            show(vendored_field),
                            show(live_field)
                        ));
                    }
                }
            }
        }
        if lines.is_empty() {
            "  (the documents differ in a way this walker did not localise)".to_owned()
        } else {
            lines.join("\n")
        }
    }

    #[test]
    fn the_vendored_contract_is_the_pinned_confitures_exit_codes_json() {
        // The cross-repo freshness check, and the one that has to compare the WHOLE
        // document. Reducing it to the integer→class map is what let fraisier-core carry a
        // table stale in eight of nine entries while both its guards stayed green
        // (fraisier-core#63), and a guard that skips when the tool is absent is a guard that
        // has never run. A missing or unpinned confiture is a failure here, never a skip.
        let pin = pinned::version();
        let live: serde_json::Value = serde_json::from_str(&pinned::output(&["--exit-codes-json"]))
            .expect("confiture emits JSON");
        assert!(
            live.get("mappings").is_none(),
            "confiture {pin} now emits a `mappings` key of its own, so the face's section can no \
             longer be told from the tool's — rename ours before regenerating"
        );

        // The face's own section is lifted out and the rest is held to the tool whole, so a
        // key confiture adds or drops still fails here.
        let mut vendored: serde_json::Value =
            serde_json::from_str(VENDORED).expect("the vendored document parses");
        let lifted = vendored.as_object_mut().and_then(|document| document.remove("mappings"));
        assert!(
            lifted.is_some(),
            "the vendored document carries no `mappings` section — the per-tool mapping is \
             supposed to live inside it, not in a `match` statement (D7)"
        );
        assert_eq!(
            live,
            vendored,
            "the confiture half of exit_table.vendored.json is not what confiture {pin} emits. It \
             drifts at:\n{}\nRegenerate it with the command in this module's docs.",
            describe_drift(&live, &vendored)
        );
    }
}
