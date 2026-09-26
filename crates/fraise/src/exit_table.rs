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

/// Confiture's exit-code contract, with the per-tool mapping that turns another tool's
/// raw exit into one of its classes.
///
/// Obtained from [`ExitTable::vendored`]; there is no other table.
#[derive(Debug)]
pub struct ExitTable {
    exits: BTreeMap<i32, ExitEntry>,
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

    /// The class of that name, or `None` when the contract defines none.
    ///
    /// Crate-private, and only for a document of ours that names a class — the compatibility
    /// table names the class it refuses with, and resolving it here is what keeps the
    /// umbrella's one exit taxonomy in this contract. A caller outside holds an [`ExitClass`]
    /// rather than a name.
    pub(crate) fn class_named(&self, name: &str) -> Option<ExitClass<'_>> {
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
    /// a dispatch cannot load at all: every class an exit or a mapping row names is one
    /// the contract lists, no class is given to two exits (or reading a class back would
    /// be ambiguous), every class has an exit, and no tool maps the same
    /// `(exit, error class)` twice.
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
    tools: BTreeMap<String, ToolMapping>,
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
    use super::{ExitTable, VENDORED};

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
        // Cycle 4 refuses to dispatch to a tool the table has nothing to say about, so an
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
    fn a_row_without_its_reason_is_refused() {
        // `why` is required so that no exit acquires a reading nobody had to justify.
        let source = doctored(|document| {
            let row = &mut document["mappings"]["tools"]["fraisier"]["rows"][1];
            row.as_object_mut().expect("a row is an object").remove("why");
        });
        let error = ExitTable::parse(&source).expect_err("the row gives no reason");
        assert!(error.contains("why"), "the refusal should name the missing field: {error}");
    }

    /// The confiture release the contract is measured against, read out of
    /// `tools/confiture-requirements.txt` so the pin has exactly one home. The test below
    /// asserts the tool it runs reports *this* version, which couples a pin bump to a
    /// regeneration of the document in the same commit — and makes "too old to have the
    /// flag" a named failure instead of a silent pass.
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

    /// The confiture to measure: `FRAISE_CONFITURE_BIN` when set, otherwise `confiture` on
    /// `PATH`, which is where CI puts the pinned one.
    fn confiture_program() -> std::ffi::OsString {
        std::env::var_os("FRAISE_CONFITURE_BIN")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| std::ffi::OsString::from("confiture"))
    }

    /// Quoted in every failure below, so a red checkout is three commands from green.
    fn install_hint(pin: &str) -> String {
        format!(
            "the exit-code contract is measured against confiture {pin}. Install it:\n  \
             uv venv --python 3.11 /tmp/confiture\n  \
             uv pip install --python /tmp/confiture/bin/python -r \
             tools/confiture-requirements.txt\n  \
             PATH=/tmp/confiture/bin:$PATH cargo xtask ci\n\
             (CI installs the same pin and puts it on PATH before the gate.)"
        )
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
        let pin = pinned_confiture_version();
        let program = confiture_program();
        let shown = program.to_string_lossy().into_owned();

        let version = match std::process::Command::new(&program).arg("--version").output() {
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
        // `confiture --version` opens with `confiture version <semver>`; its later lines
        // report the parser build and native extension, which are the machine's rather than
        // the release's, so only the first line is the contract.
        assert_eq!(
            version,
            format!("confiture version {pin}"),
            "this is a different confiture, so any diff below would be the wrong release's.\n{}",
            install_hint(&pin)
        );

        let output = std::process::Command::new(&program)
            .arg("--exit-codes-json")
            .output()
            .unwrap_or_else(|error| panic!("running `{shown} --exit-codes-json`: {error}"));
        assert!(
            output.status.success(),
            "`{shown} --exit-codes-json` exited {:?}",
            output.status.code()
        );
        let live: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("confiture emits JSON");
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
