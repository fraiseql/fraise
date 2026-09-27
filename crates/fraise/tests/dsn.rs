//! How `fraise` finds the database: confiture's precedence contract, through this face's inputs.
//!
//! Three of the four tools want a DSN and each reads it its own way, so the umbrella has to
//! answer one question before it runs any of them — *which database is this invocation about* —
//! and answer it the same way every time. The answer is not invented here: confiture's #152
//! precedence contract already decides it, in `python/confiture/cli/dsn.py::resolve_database_url`,
//! whose principle is written there as **explicit-and-singular wins; ambiguity fails loud**. That
//! file is byte-identical in the pinned 1.19.0 and in the 1.24.0 checkout, so the ladder below is
//! a contract rather than a release's behaviour.
//!
//! `fraise`'s inputs are not confiture's — it has no `--database-url`, because a DSN never
//! reaches argv, and its config is one TOML document rather than a YAML per environment — so
//! each row of the table says which of confiture's eight steps it mirrors. Where the two
//! deliberately differ, the row says that too.
//!
//! Every case runs the built binary with an environment holding nothing but what it says it
//! holds. An ambient variable is not a hypothetical: one `FRAISER_*` in the environment reddened
//! seven unrelated tests in fraisier-core (#64), and a precedence rule is exactly the kind of
//! thing an ambient variable makes pass.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The exit a refusal about configuration carries: confiture's `invalid_config` written as the
/// integer its frozen contract gives that class.
///
/// It is the class confiture's own ladder raises for both of the refusals adopted here —
/// `CONFIG_007` for two explicit sources and `CONFIG_010` for no usable one — which
/// [`the_refusals_are_the_class_confitures_own_ladder_raises`] measures against the vendored
/// document rather than asserting from memory.
const REFUSED: i32 = 5;

/// A project whose document declares two environments and defaults to one of them.
const TWO: &str = r#"
[project]
name = "printoptim"
default_environment = "local"

[environments.local]
database_url_env = "PRINTOPTIM_LOCAL_DATABASE_URL"

[environments.staging]
database_url_env = "PRINTOPTIM_STAGING_DATABASE_URL"
"#;

/// The same two environments, and no default: nothing in the document says which one an
/// invocation is about, so the rungs below a document's own statement become reachable.
const NO_DEFAULT: &str = r#"
[project]
name = "printoptim"

[environments.local]
database_url_env = "PRINTOPTIM_LOCAL_DATABASE_URL"

[environments.staging]
database_url_env = "PRINTOPTIM_STAGING_DATABASE_URL"
"#;

/// A document that says nothing about any database.
const MINIMAL: &str = "[project]\nname = \"printoptim\"\n";

/// A DSN no report, refusal or envelope may print.
const DSN: &str = "postgresql://printoptim:s3cret@db.internal:5432/printoptim";

/// One case: a project directory, and `fraise` run in it.
struct Case {
    root: PathBuf,
}

impl Case {
    /// A project whose `fraise.toml` is `document`, with a `bin/` for stubs.
    fn new(name: &str, document: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dsn").join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("bin")).expect("the case directory is created");
        fs::write(root.join("fraise.toml"), document).expect("the document is written");
        Self { root }
    }

    /// A stub for `program` that answers `--version` with `version_line`, and on any other
    /// invocation records the DSN variables it was given, then exits 0.
    ///
    /// Only shell builtins: `PATH` holds the stub directory alone when these run.
    fn tool(&self, program: &str, version_line: &str) {
        let path = self.root.join("bin").join(program);
        let sentinel = self.root.join(format!("{program}.ran"));
        let script = format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then\n\
             echo '{version_line}'\n\
             exit 0\n\
             fi\n\
             {{ echo \"CONFITURE_DATABASE_URL=[${{CONFITURE_DATABASE_URL:-}}]\"\n\
             echo \"DATABASE_URL=[${{DATABASE_URL:-}}]\"\n\
             echo \"PRINTOPTIM_LOCAL_DATABASE_URL=[${{PRINTOPTIM_LOCAL_DATABASE_URL:-}}]\"\n\
             echo \"PRINTOPTIM_STAGING_DATABASE_URL=[${{PRINTOPTIM_STAGING_DATABASE_URL:-}}]\"\n\
             echo \"OTHER_DATABASE_URL=[${{OTHER_DATABASE_URL:-}}]\"; }} >'{sentinel}'\n\
             exit 0\n",
            sentinel = sentinel.display()
        );
        fs::write(&path, script).expect("the stub is written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("the stub is made executable");
    }

    /// What the stub was handed, or `None` when it never ran.
    fn ran(&self, program: &str) -> Option<String> {
        fs::read_to_string(self.root.join(format!("{program}.ran"))).ok()
    }

    /// `fraise`, run in this project with an environment holding `vars`, this case's stubs, and
    /// nothing else.
    fn fraise(&self, args: &[&str], vars: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fraise"));
        command
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", self.root.join("bin"));
        for (name, value) in vars {
            command.env(name, value);
        }
        command.output().expect("the built binary runs")
    }

    /// What `config show --json` says about the database, for an invocation given `args` in an
    /// environment holding `vars`.
    fn database(&self, args: &[&str], vars: &[(&str, &str)]) -> serde_json::Value {
        let mut whole = vec!["--json", "config", "show"];
        whole.extend_from_slice(args);
        let output = self.fraise(&whole, vars);
        assert_eq!(output.status.code(), Some(0), "the document resolves: {}", shown(&output));
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let envelope: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("{error}: {stdout}"));
        envelope["payload"].clone()
    }
}

fn shown(output: &Output) -> String {
    format!(
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// What one row of the ladder expects.
enum Expect {
    /// The rung that answers, the environment in force, the variable that carries the DSN, and
    /// whether that variable is set where `fraise` is running.
    Rung {
        rung: &'static str,
        environment: Option<&'static str>,
        variable: Option<&'static str>,
        set: bool,
    },
    /// A refusal naming each of these.
    Refused(&'static [&'static str]),
}

/// One row: what is stated, and what the ladder makes of it.
struct Row {
    /// Which of confiture's eight numbered steps this row mirrors, and what it is about.
    about: &'static str,
    document: &'static str,
    args: &'static [&'static str],
    vars: &'static [(&'static str, &'static str)],
    expect: Expect,
}

/// The ladder, row by row, against the binary.
///
/// Each row names the step of confiture's contract it mirrors. The order of the rungs is the
/// order of those steps, and the rows that refuse are the two refusals confiture's own resolver
/// raises: two explicit sources (`CONFIG_007`) and no usable source for a mutating command
/// (`CONFIG_010`, exercised through `fraise tool` below, since `config show` mutates nothing).
const LADDER: &[Row] = &[
    Row {
        about: "step 1: a variable named on argv always wins, as confiture's --database-url does",
        document: TWO,
        args: &["--database-url-env", "OTHER_DATABASE_URL"],
        vars: &[("OTHER_DATABASE_URL", DSN)],
        expect: Expect::Rung {
            rung: "named_variable",
            environment: Some("local"),
            variable: Some("OTHER_DATABASE_URL"),
            set: true,
        },
    },
    Row {
        about: "step 1 again: it wins over the two explicit statements that conflict with each \
                other, exactly as confiture's flag returns before its CONFIG_007 check",
        document: TWO,
        args: &[
            "--database-url-env",
            "OTHER_DATABASE_URL",
            "--environment",
            "staging",
        ],
        vars: &[("OTHER_DATABASE_URL", DSN), ("CONFITURE_DATABASE_URL", DSN)],
        expect: Expect::Rung {
            rung: "named_variable",
            environment: Some("staging"),
            variable: Some("OTHER_DATABASE_URL"),
            set: true,
        },
    },
    Row {
        about: "step 4: an environment the caller chose, and an unset variable is still the one \
                it chose — the ladder does not look elsewhere",
        document: TWO,
        args: &["--environment", "staging"],
        vars: &[],
        expect: Expect::Rung {
            rung: "chosen_environment",
            environment: Some("staging"),
            variable: Some("PRINTOPTIM_STAGING_DATABASE_URL"),
            set: false,
        },
    },
    Row {
        about: "step 3: an explicit environment and the canonical variable are two explicit \
                sources, and two are never reconciled silently (CONFIG_007)",
        document: TWO,
        args: &["--environment", "staging"],
        vars: &[("CONFITURE_DATABASE_URL", DSN)],
        expect: Expect::Refused(&["--environment", "CONFITURE_DATABASE_URL"]),
    },
    Row {
        about: "step 5: the canonical variable is set on purpose, so it beats a document that \
                merely defaults",
        document: TWO,
        args: &[],
        vars: &[("CONFITURE_DATABASE_URL", DSN)],
        expect: Expect::Rung {
            rung: "canonical_variable",
            environment: Some("local"),
            variable: Some("CONFITURE_DATABASE_URL"),
            set: true,
        },
    },
    Row {
        about: "step 5, and confiture's own truthiness: an empty variable is not a source, so \
                the document's default answers",
        document: TWO,
        args: &[],
        vars: &[("CONFITURE_DATABASE_URL", "")],
        expect: Expect::Rung {
            rung: "default_environment",
            environment: Some("local"),
            variable: Some("PRINTOPTIM_LOCAL_DATABASE_URL"),
            set: false,
        },
    },
    Row {
        about: "step 6: the document's own default beats an ambient DATABASE_URL",
        document: TWO,
        args: &[],
        vars: &[("DATABASE_URL", DSN)],
        expect: Expect::Rung {
            rung: "default_environment",
            environment: Some("local"),
            variable: Some("PRINTOPTIM_LOCAL_DATABASE_URL"),
            set: false,
        },
    },
    Row {
        about: "step 7: nothing intentional is stated, so the ambient variable is what is left — \
                and it is marked unintentional, which is what a mutating command refuses",
        document: NO_DEFAULT,
        args: &[],
        vars: &[("DATABASE_URL", DSN)],
        expect: Expect::Rung {
            rung: "ambient_variable",
            environment: None,
            variable: Some("DATABASE_URL"),
            set: true,
        },
    },
    Row {
        about: "step 8: no source at all, which is an answer and not a refusal — the tool's own \
                configuration may still hold one",
        document: MINIMAL,
        args: &[],
        vars: &[],
        expect: Expect::Rung {
            rung: "nothing",
            environment: None,
            variable: None,
            set: false,
        },
    },
    Row {
        about: "the FRAISE_ override is the flag's other spelling: it names a source, never a \
                value",
        document: TWO,
        args: &[],
        vars: &[("FRAISE_ENVIRONMENT", "staging")],
        expect: Expect::Rung {
            rung: "chosen_environment",
            environment: Some("staging"),
            variable: Some("PRINTOPTIM_STAGING_DATABASE_URL"),
            set: false,
        },
    },
    Row {
        about: "and argv beats it, because a flag is the more explicit spelling of the same \
                statement",
        document: TWO,
        args: &["--environment", "local"],
        vars: &[("FRAISE_ENVIRONMENT", "staging")],
        expect: Expect::Rung {
            rung: "chosen_environment",
            environment: Some("local"),
            variable: Some("PRINTOPTIM_LOCAL_DATABASE_URL"),
            set: false,
        },
    },
    Row {
        about: "the other override names the variable rather than the environment",
        document: TWO,
        args: &[],
        vars: &[
            ("FRAISE_DATABASE_URL_ENV", "OTHER_DATABASE_URL"),
            ("OTHER_DATABASE_URL", DSN),
        ],
        expect: Expect::Rung {
            rung: "named_variable",
            environment: Some("local"),
            variable: Some("OTHER_DATABASE_URL"),
            set: true,
        },
    },
    Row {
        about: "an environment the document does not declare is refused with the ones it does, \
                as a default_environment naming no environment already is",
        document: TWO,
        args: &["--environment", "prod"],
        vars: &[],
        expect: Expect::Refused(&["prod", "local", "staging"]),
    },
    Row {
        about: "and in a document that declares none, the refusal says so rather than listing \
                nothing",
        document: MINIMAL,
        args: &["--environment", "local"],
        vars: &[],
        expect: Expect::Refused(&["local", "declares none"]),
    },
    Row {
        about: "a variable's name is held to the form confiture expands, wherever it is written",
        document: TWO,
        args: &["--database-url-env", "postgresql://user@host/db"],
        vars: &[],
        expect: Expect::Refused(&["--database-url-env"]),
    },
];

#[test]
fn the_ladder_is_confitures_precedence_contract_through_this_faces_inputs() {
    for (index, row) in LADDER.iter().enumerate() {
        let case = Case::new(&format!("ladder-{index}"), row.document);
        match &row.expect {
            Expect::Rung {
                rung,
                environment,
                variable,
                set,
            } => {
                let payload = case.database(row.args, row.vars);
                let database = &payload["database"];
                assert_eq!(database["rung"], serde_json::json!(rung), "{}: {database:#}", row.about);
                assert_eq!(
                    database["environment"],
                    environment.map_or(serde_json::Value::Null, |name| serde_json::json!(name)),
                    "{}: {database:#}",
                    row.about
                );
                assert_eq!(
                    database["variable"],
                    variable.map_or(serde_json::Value::Null, |name| serde_json::json!(name)),
                    "{}: {database:#}",
                    row.about
                );
                assert_eq!(database["set"], serde_json::json!(set), "{}: {database:#}", row.about);
                assert!(
                    !payload.to_string().contains("s3cret"),
                    "{}: a DSN reached the report: {payload:#}",
                    row.about
                );
            },
            Expect::Refused(named) => {
                let mut whole = vec!["config", "show"];
                whole.extend_from_slice(row.args);
                let output = case.fraise(&whole, row.vars);
                assert_eq!(
                    output.status.code(),
                    Some(REFUSED),
                    "{}: {}",
                    row.about,
                    shown(&output)
                );
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                for expected in *named {
                    assert!(stderr.contains(expected), "{}: {stderr}", row.about);
                }
                assert!(!stderr.contains("s3cret"), "{}: {stderr}", row.about);
            },
        }
    }
}

#[test]
fn an_override_that_decided_the_source_is_reported_as_coming_from_the_environment() {
    // Cycle 1's rule: what a reader is shown is the document as written, and `from_env` says
    // which variable fed what. An override is a statement from the environment, so it owes the
    // same entry — otherwise the report would show a document that is not the one in force.
    let case = Case::new("override-from-env", TWO);

    let by_environment = case.database(&[], &[("FRAISE_ENVIRONMENT", "staging")]);
    assert_eq!(
        by_environment["from_env"]["database.environment"],
        serde_json::json!(["FRAISE_ENVIRONMENT"]),
        "{by_environment:#}"
    );

    let by_variable = case.database(
        &[],
        &[
            ("FRAISE_DATABASE_URL_ENV", "OTHER_DATABASE_URL"),
            ("OTHER_DATABASE_URL", DSN),
        ],
    );
    assert_eq!(
        by_variable["from_env"]["database.variable"],
        serde_json::json!(["FRAISE_DATABASE_URL_ENV"]),
        "{by_variable:#}"
    );

    // A flag is not the environment, so nothing is owed for one.
    let by_flag = case.database(&["--environment", "staging"], &[]);
    assert_eq!(
        by_flag["from_env"]["database.environment"],
        serde_json::Value::Null,
        "{by_flag:#}"
    );
}

#[test]
fn the_dsn_reaches_the_tool_under_every_name_the_stack_reads_it_by() {
    // The translation this face exists to do. One DSN for the invocation, reachable under the
    // canonical name confiture reads (`CONFITURE_DATABASE_URL`, its #152 contract), the
    // conventional one fraiseql reads (`DATABASE_URL`, fraiseql-cli/src/commands/run.rs), and
    // the name the project's own document gave it — which is the name fraisier resolves through
    // its config (`[migration].database_url_env`, its Decision 5). Two tools reading two
    // databases inside one command is the failure this prevents.
    let case = Case::new("handover", TWO);
    case.tool("confiture", "confiture version 1.19.0");

    let output = case.fraise(
        &["tool", "confiture", "migrate", "status"],
        &[("PRINTOPTIM_LOCAL_DATABASE_URL", DSN)],
    );
    assert!(output.status.success(), "the dispatch proceeds: {}", shown(&output));

    let handed = case.ran("confiture").expect("the verb reached the tool");
    for name in [
        "CONFITURE_DATABASE_URL",
        "DATABASE_URL",
        "PRINTOPTIM_LOCAL_DATABASE_URL",
    ] {
        assert!(handed.contains(&format!("{name}=[{DSN}]")), "{name} was not handed over: {handed}");
    }
    assert!(
        handed.contains("PRINTOPTIM_STAGING_DATABASE_URL=[]"),
        "and no other environment's variable was invented: {handed}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("s3cret")
            && !String::from_utf8_lossy(&output.stderr).contains("s3cret"),
        "the DSN is passed and never printed: {}",
        shown(&output)
    );
}

#[test]
fn an_ambient_dsn_is_never_promoted_to_an_intentional_one() {
    // The asymmetry is confiture's own and it is the whole point of #152: the canonical
    // variable is set on purpose, the ambient one is whatever the shell happens to hold. Were
    // `fraise` to copy an ambient DATABASE_URL into CONFITURE_DATABASE_URL, confiture's own
    // refusal for a mutating command would never fire again — the face would have laundered an
    // accident into an intention.
    let case = Case::new("no-promotion", NO_DEFAULT);
    case.tool("confiture", "confiture version 1.19.0");

    let output = case.fraise(&["tool", "confiture", "migrate", "status"], &[("DATABASE_URL", DSN)]);
    assert!(output.status.success(), "a reading proceeds: {}", shown(&output));

    let handed = case.ran("confiture").expect("the verb reached the tool");
    assert!(handed.contains(&format!("DATABASE_URL=[{DSN}]")), "it is still there: {handed}");
    assert!(
        handed.contains("CONFITURE_DATABASE_URL=[]"),
        "and it was not promoted: {handed}"
    );
}

#[test]
fn a_mutating_invocation_refuses_an_ambient_dsn_and_says_what_would_name_one() {
    // Confiture's step 7 with `require_intentional_source`, which is its own name for "a
    // mutating command refuses an ambient-only DSN". `fraise` cannot read a tool's arguments to
    // know whether they mutate — it will not read a tool's flags on its behalf — so the caller
    // that wrote them says, exactly as `--payload` is the caller's to say.
    let case = Case::new("mutating-ambient", NO_DEFAULT);
    case.tool("confiture", "confiture version 1.19.0");

    let output = case.fraise(
        &["tool", "--mutating", "confiture", "migrate", "up"],
        &[("DATABASE_URL", DSN)],
    );
    assert_eq!(
        output.status.code(),
        Some(REFUSED),
        "an ambient DSN is no source for a mutation: {}",
        shown(&output)
    );
    assert_eq!(case.ran("confiture"), None, "and it is refused before the tool runs");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(stderr.contains("DATABASE_URL"), "the refusal names what it would not use: {stderr}");
    assert!(
        stderr.contains("--environment") && stderr.contains("--database-url-env"),
        "and what would name a source instead: {stderr}"
    );
    assert!(!stderr.contains("s3cret"), "without printing the DSN: {stderr}");
}

#[test]
fn a_mutating_invocation_runs_against_a_source_that_was_named() {
    // The other half: the refusal is about ambience, not about mutating.
    let case = Case::new("mutating-named", TWO);
    case.tool("confiture", "confiture version 1.19.0");

    let output = case.fraise(
        &["tool", "--mutating", "confiture", "migrate", "up"],
        &[("PRINTOPTIM_LOCAL_DATABASE_URL", DSN)],
    );
    assert!(output.status.success(), "a named source is a source: {}", shown(&output));
    assert!(case.ran("confiture").is_some(), "and the verb ran");
}

#[test]
fn a_variable_the_document_names_and_the_machine_does_not_set_is_refused_at_the_exec() {
    // Judgement: looking is not reading. `config show` reports that the variable is not set and
    // exits 0, because whether a machine exports a DSN is a fact about the machine at this
    // moment and not about the document. The refusal belongs where the DSN is actually needed,
    // and it names the rung that chose the variable — because the fix is either to set it or to
    // name another source.
    let case = Case::new("unset-at-exec", TWO);
    case.tool("confiture", "confiture version 1.19.0");

    let output = case.fraise(&["tool", "confiture", "migrate", "status"], &[]);
    assert_eq!(
        output.status.code(),
        Some(REFUSED),
        "the source the document named is not there: {}",
        shown(&output)
    );
    assert_eq!(case.ran("confiture"), None, "and nothing ran against something else");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stderr.contains("PRINTOPTIM_LOCAL_DATABASE_URL"),
        "the refusal names the variable: {stderr}"
    );
    assert!(stderr.contains("local"), "and the environment that named it: {stderr}");
}

#[test]
fn what_the_document_names_is_shown_to_a_person_without_the_value() {
    // One rule about what is printed, not one per output mode.
    let case = Case::new("rendered", TWO);
    let output = case.fraise(&["config", "show"], &[("PRINTOPTIM_LOCAL_DATABASE_URL", DSN)]);
    assert_eq!(output.status.code(), Some(0), "the document resolves: {}", shown(&output));

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(!stdout.contains("s3cret"), "the DSN stays out of the report: {stdout}");
    assert!(
        stdout.contains("PRINTOPTIM_LOCAL_DATABASE_URL"),
        "the variable is named: {stdout}"
    );
    assert!(stdout.contains("local"), "and the environment in force: {stdout}");
}

#[test]
fn the_refusals_are_the_class_confitures_own_ladder_raises() {
    // The exit above is not chosen, it is adopted: confiture's resolver raises CONFIG_007 for
    // two explicit sources and CONFIG_010 for no usable one, and the frozen contract lists both
    // under one class. This reads the vendored document rather than trusting the number, so a
    // contract that moved those codes would redden here instead of silently giving this face's
    // refusals a different meaning from the tool whose rules they are.
    let document = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/exit_table.vendored.json"),
    )
    .expect("the vendored contract is readable");
    let contract: serde_json::Value =
        serde_json::from_str(&document).expect("the vendored contract parses");
    let codes = &contract["exit_codes"][REFUSED.to_string()]["symbolic_codes"];
    for code in ["CONFIG_007", "CONFIG_010"] {
        assert!(
            codes.as_array().is_some_and(|listed| listed.iter().any(|it| it == code)),
            "{code} is not listed under exit {REFUSED}: {codes:#}"
        );
    }
}
