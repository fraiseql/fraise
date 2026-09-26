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
//! drifts in silence; a document is diffed. Every class a mapping row names must be one
//! the confiture half lists, which is what keeps the two halves one table rather than
//! two, and is why no confiture class string is ever written in this crate's source: a
//! class is a `&str` borrowed from the document and nothing else.
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

use std::sync::OnceLock;

use serde_json::Value;

/// The contract document, compiled into the binary so a released `fraise` carries the
/// same table its tests measured.
const VENDORED: &str = include_str!("exit_table.vendored.json");

/// Confiture's exit-code contract, with the per-tool mapping that turns another tool's
/// raw exit into one of its classes.
///
/// Obtained from [`ExitTable::vendored`]; there is no other constructor, because there is
/// no other table.
#[derive(Debug)]
pub struct ExitTable {
    document: Value,
}

impl ExitTable {
    /// The vendored table, parsed once per process.
    ///
    /// # Panics
    ///
    /// If the vendored document is not JSON. It is compiled in, so that is a build the
    /// tests below would have failed before it ever shipped.
    #[must_use]
    pub fn vendored() -> &'static Self {
        static TABLE: OnceLock<ExitTable> = OnceLock::new();
        TABLE.get_or_init(|| Self {
            document: serde_json::from_str(VENDORED).expect("the vendored document is JSON"),
        })
    }

    /// The whole document, for the freshness comparison — the only caller that wants the
    /// table unprojected, because it is the one that holds it to the tool.
    #[must_use]
    pub const fn document(&self) -> &Value {
        &self.document
    }

    /// The semantic class confiture gives its own exit `exit`, or `None` for an exit it
    /// does not document.
    #[must_use]
    pub fn class_of_exit(&self, exit: i32) -> Option<&str> {
        self.document["exit_codes"][exit.to_string()]["class"].as_str()
    }

    /// The confiture exit integer whose class is `class`, or `None` when no class of that
    /// name is in the table. The two are one-to-one, so this is the inverse of
    /// [`class_of_exit`](Self::class_of_exit).
    #[must_use]
    pub fn exit_of_class(&self, class: &str) -> Option<i32> {
        self.document["exit_codes"]
            .as_object()?
            .iter()
            .find(|(_, entry)| entry["class"].as_str() == Some(class))
            .and_then(|(exit, _)| exit.parse().ok())
    }

    /// The confiture class that `tool`'s exit `tool_exit` means.
    ///
    /// `error_class` is the tool's own error taxonomy name when the tool reports one, and
    /// `None` when it does not — which is every tool today. A row for the exact pair wins;
    /// otherwise the exit's `error_class: null` row answers; otherwise the tool's
    /// `unlisted` fallback does. `None` means the table knows no such tool, which is not a
    /// classification but a refusal.
    #[must_use]
    pub fn classify(&self, tool: &str, tool_exit: i32, error_class: Option<&str>) -> Option<&str> {
        let entry = &self.document["mappings"]["tools"][tool];
        let rows = entry["rows"].as_array()?;
        let same_exit = |row: &&Value| row["tool_exit"].as_i64() == Some(i64::from(tool_exit));
        let exact = error_class.and_then(|class| {
            rows.iter()
                .find(|row| same_exit(row) && row["error_class"].as_str() == Some(class))
        });
        exact
            .or_else(|| rows.iter().find(|row| same_exit(row) && row["error_class"].is_null()))
            .and_then(|row| row["class"].as_str())
            .or_else(|| entry["unlisted"]["class"].as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{ExitTable, VENDORED};

    /// D7's mapping, written in the decision's own vocabulary — a tool's raw exit, the
    /// error class it reports when it reports one, and the **confiture exit integer** the
    /// face reads it as. Integers rather than class names on the right-hand side so this
    /// table is an independent statement of the decision ("fraiseql 2 → 5, specql 1 → 4/5
    /// by class, fraisier 1 → 1") and not a second reading of the document under test.
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
        assert!(
            table.document().get("mappings").is_some(),
            "the vendored document carries no `mappings` section — the per-tool mapping is \
             supposed to live inside it, not in a `match` statement (D7)"
        );
        for (tool, tool_exit, error_class, confiture_exit) in MAPPING {
            let class = table.classify(tool, *tool_exit, *error_class).unwrap_or_else(|| {
                panic!("the table maps no exit of {tool}: classify({tool_exit}, {error_class:?})")
            });
            assert_eq!(
                table.exit_of_class(class),
                Some(*confiture_exit),
                "{tool} exit {tool_exit} (error class {error_class:?}) reads as {class}, and the \
                 mapping says it should read as confiture's exit {confiture_exit}"
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
    fn every_class_a_mapping_row_names_is_one_the_contract_lists() {
        // This is what keeps the two halves of the document one table. A row naming a class
        // confiture does not define would classify into nothing, and `exit_of_class` would
        // return `None` on a live dispatch rather than at rest.
        let table = ExitTable::vendored();
        let document = table.document();
        let classes: Vec<&str> = document["classes"]
            .as_array()
            .expect("the contract lists its classes")
            .iter()
            .map(|class| class.as_str().expect("a class is a string"))
            .collect();
        // The confiture half is one-to-one: nine classes, nine exits, no class unreachable.
        for class in &classes {
            assert!(
                table.exit_of_class(class).is_some(),
                "the contract names the class {class} and gives it no exit"
            );
        }
        let tools = document["mappings"]["tools"]
            .as_object()
            .expect("the mapping names the tools it maps");
        for (tool, entry) in tools {
            let named = entry["rows"]
                .as_array()
                .unwrap_or_else(|| panic!("{tool} has no mapping rows"))
                .iter()
                .map(|row| &row["class"])
                .chain(std::iter::once(&entry["unlisted"]["class"]));
            for class in named {
                let class =
                    class.as_str().unwrap_or_else(|| panic!("{tool} names a non-string class"));
                assert!(
                    classes.contains(&class),
                    "{tool} maps onto {class}, which confiture's half of the document does not list"
                );
            }
        }
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
        // (fraisier-core#63), and a guard that skips when the tool is absent is a guard
        // that has never run. A missing or unpinned confiture is a failure here, never a
        // skip.
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
            "confiture {pin} now emits a `mappings` key of its own, so the face's section can \
             no longer be told from the tool's — rename ours before regenerating"
        );

        // The face's own section is lifted out and the rest is held to the tool whole, so a
        // key confiture adds or drops still fails here.
        let mut vendored: serde_json::Value =
            serde_json::from_str(VENDORED).expect("the vendored document parses");
        if let Some(document) = vendored.as_object_mut() {
            document.remove("mappings");
        }
        assert_eq!(
            live,
            vendored,
            "the confiture half of exit_table.vendored.json is not what confiture {pin} emits. \
             It drifts at:\n{}\nRegenerate it with the command in this module's docs.",
            describe_drift(&live, &vendored)
        );
    }
}
