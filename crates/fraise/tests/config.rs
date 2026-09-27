//! `fraise.toml`, the one file a project author writes.
//!
//! Four tools read four configurations today, and three of them want a DSN. This file replaces
//! all of that, which puts two promises on its loader. It has to refuse a document rather than
//! half-read one — a key nothing consumes, a `${VAR}` in a form confiture would not expand, a
//! variable that is not set — because a configuration that loads and means something else is
//! how a tool succeeds at doing nothing. And it has to keep the DSN out of the file: an
//! environment names the *variable* that carries its connection string, never the string.
//!
//! So every case here is a refusal or a rendering, both driven through the built binary rather
//! than through the loader's own API: what a person and an agent get is the exit, the sentence
//! on standard error and the envelope, and those are only true of the binary.
//!
//! Each case runs with an environment holding nothing but what it says it holds. An ambient
//! variable is not a hypothetical: one `FRAISER_*` in the environment reddened seven unrelated
//! tests in fraisier-core (#64), and a `${VAR}` rule is exactly the kind of thing an ambient
//! variable makes pass.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The exit a refusal about configuration carries: confiture's `invalid_config` written as the
/// integer its frozen contract gives that class, which is the plan's "exit 5".
///
/// The crate's source names that class and never this number; this test names the number and
/// never the class, so the two are an independent statement of one decision rather than two
/// readings of the same file.
const REFUSED: i32 = 5;

/// A document with one of everything the loader reads, and one value that comes from the
/// environment. The cases below start from it and change the one thing each is about.
const WHOLE: &str = r#"
[project]
name = "printoptim"
default_environment = "local"

[environments.local]
database_url_env = "PRINTOPTIM_LOCAL_DATABASE_URL"

[environments.staging]
database_url_env = "PRINTOPTIM_STAGING_DATABASE_URL"

[confiture]
migrations_dir = "db/migrations"
notify_url = "${WEBHOOK_URL}"

[fraiseql]
schema = "public"

[fraisier]
strategy = "blue-green"

[specql]
spec = "spec/printoptim.toml"
"#;

/// The variable `WHOLE` refers to, and a value no report may print.
const WEBHOOK: (&str, &str) = ("WEBHOOK_URL", "https://hooks.example/s3cret-token");

/// One case: a directory that is a project, and `fraise` run in it.
struct Case {
    root: PathBuf,
}

impl Case {
    /// A project whose `fraise.toml` is `document`.
    fn new(name: &str, document: &str) -> Self {
        let case = Self::bare(name);
        fs::write(case.root.join("fraise.toml"), document).expect("the document is written");
        case
    }

    /// A directory that is not a project at all.
    fn bare(name: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("config").join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the case directory is created");
        Self { root }
    }

    /// `fraise`, run in this project with an environment holding `vars` and nothing else.
    fn fraise(&self, args: &[&str], vars: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fraise"));
        command.args(args).current_dir(&self.root).env_clear();
        for (name, value) in vars {
            command.env(name, value);
        }
        command.output().expect("the built binary runs")
    }

    /// The refusal this document earns: the exit, and what was said about it.
    fn refusal(&self, vars: &[(&str, &str)]) -> String {
        let output = self.fraise(&["config", "show"], vars);
        assert_eq!(
            output.status.code(),
            Some(REFUSED),
            "a document fraise cannot act on is an invalid configuration: {}",
            shown(&output)
        );
        String::from_utf8_lossy(&output.stderr).into_owned()
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

#[test]
fn a_key_nothing_reads_is_refused_and_named() {
    // A tolerated unknown key is a setting that silently does nothing, which for a document
    // that configures four tools means a migration run against the wrong thing. The key is
    // named because the author's next move is to fix that line.
    let stderr = Case::new("unknown-key", &WHOLE.replace("name = \"printoptim\"", "nmae = \"x\""))
        .refusal(&[WEBHOOK]);
    assert!(stderr.contains("nmae"), "the refusal names the key: {stderr}");
    assert!(stderr.contains("fraise.toml"), "the refusal names the file: {stderr}");
}

#[test]
fn a_reference_confiture_would_not_expand_is_refused_here_too() {
    // `${lower}` is confiture's own near-miss case: it scans for anything shaped like a
    // reference and refuses what does not match `[A-Z_][A-Z0-9_]*`, rather than leaving it in
    // the value. `fraise` writes confiture's YAML, so a form this file accepted and confiture
    // rejected would be a document that loads here and fails one layer down.
    let stderr = Case::new("lowercase", &WHOLE.replace("${WEBHOOK_URL}", "${webhook_url}"))
        .refusal(&[WEBHOOK]);
    assert!(stderr.contains("webhook_url"), "the refusal quotes the reference: {stderr}");
    assert!(
        stderr.contains("confiture.notify_url"),
        "the refusal names where in the document it is: {stderr}"
    );
}

#[test]
fn a_variable_that_is_not_set_is_refused_rather_than_expanded_to_nothing() {
    // Confiture's rule in one sentence: missing variables fail loud, they never expand to an
    // empty string. An empty DSN or an empty hook URL is the failure this prevents.
    let stderr = Case::new("unset", WHOLE).refusal(&[]);
    assert!(stderr.contains("WEBHOOK_URL"), "the refusal names the variable: {stderr}");
    assert!(
        stderr.contains("confiture.notify_url"),
        "the refusal names the setting that wanted it: {stderr}"
    );
}

#[test]
fn a_connection_string_where_a_variable_name_belongs_is_refused_without_being_printed() {
    // The reason this face has a config file of its own. `fraise.toml` is committed; a DSN is
    // not. The field takes the *name* of the variable that carries it, so the refusal is the
    // one rule that keeps a password out of the repository — and out of this refusal, which
    // would otherwise put it in a CI log.
    let dsn = "postgresql://printoptim:s3cret@db.internal:5432/printoptim";
    let stderr = Case::new(
        "dsn",
        &WHOLE.replace("\"PRINTOPTIM_LOCAL_DATABASE_URL\"", &format!("\"{dsn}\"")),
    )
    .refusal(&[WEBHOOK]);
    assert!(
        stderr.contains("environments.local.database_url_env"),
        "the refusal names the key: {stderr}"
    );
    assert!(
        !stderr.contains("s3cret") && !stderr.contains(dsn),
        "a refusal about a DSN must not print the DSN: {stderr}"
    );
}

#[test]
fn a_default_environment_that_names_no_environment_is_refused() {
    // Checked when the document is read rather than when a verb needs it, because the answer
    // cannot change in between and a refusal at the moment of a deploy is a refusal too late.
    let stderr = Case::new(
        "no-such-environment",
        &WHOLE.replace("default_environment = \"local\"", "default_environment = \"prod\""),
    )
    .refusal(&[WEBHOOK]);
    assert!(stderr.contains("prod"), "the refusal names what was asked for: {stderr}");
    assert!(
        stderr.contains("local") && stderr.contains("staging"),
        "and what there is to ask for: {stderr}"
    );
}

#[test]
fn a_directory_that_is_not_a_project_says_where_it_looked() {
    let case = Case::bare("no-document");
    let stderr = case.refusal(&[]);
    assert!(stderr.contains("fraise.toml"), "the refusal names the file: {stderr}");
    assert!(
        stderr.contains(&case.root.display().to_string()),
        "and the directory it looked in: {stderr}"
    );
}

#[test]
fn what_resolves_is_shown_as_the_references_the_file_holds_and_not_as_the_values() {
    // How `show` redacts: it never renders a resolved value at all. What a reader is shown is
    // the document as written, so a value that came from the environment appears as the
    // reference, and `from_env` says which variable fed which setting — which is what an agent
    // needs to act, and is not the secret.
    let case = Case::new("whole", WHOLE);
    let output = case.fraise(&["--json", "config", "show"], &[WEBHOOK]);
    assert_eq!(output.status.code(), Some(0), "the document resolves: {}", shown(&output));

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        !stdout.contains("s3cret"),
        "a value that came from the environment must not be printed: {stdout}"
    );

    let envelope: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("{error}: {stdout}"));
    assert_eq!(envelope["ok"], serde_json::json!(true), "{envelope:#}");
    assert_eq!(envelope["command"], serde_json::json!("config show"), "{envelope:#}");
    assert_eq!(envelope["tool"], serde_json::Value::Null, "no tool ran: {envelope:#}");
    assert_eq!(envelope["payload_kind"], serde_json::json!("json"), "{envelope:#}");

    let payload = &envelope["payload"];
    assert_eq!(payload["project"]["name"], serde_json::json!("printoptim"), "{payload:#}");
    assert_eq!(
        payload["project"]["default_environment"],
        serde_json::json!("local"),
        "{payload:#}"
    );
    assert_eq!(
        payload["environments"]["staging"]["database_url_env"],
        serde_json::json!("PRINTOPTIM_STAGING_DATABASE_URL"),
        "{payload:#}"
    );
    assert_eq!(
        payload["tools"]["confiture"]["notify_url"],
        serde_json::json!("${WEBHOOK_URL}"),
        "the reference is what is shown: {payload:#}"
    );
    assert_eq!(
        payload["tools"]["fraiseql"]["schema"],
        serde_json::json!("public"),
        "a passthrough table is carried whole: {payload:#}"
    );
    assert_eq!(
        payload["from_env"]["confiture.notify_url"],
        serde_json::json!(["WEBHOOK_URL"]),
        "the path and the variable that fed it: {payload:#}"
    );
}

#[test]
fn a_person_is_shown_the_same_document_and_the_same_values_are_withheld() {
    // One rule about what is printed, not one per output mode: the text report is the same
    // document as the envelope's payload, so a secret cannot leak through the mode nobody
    // wrote a test for.
    let output = Case::new("rendered", WHOLE).fraise(&["config", "show"], &[WEBHOOK]);
    assert_eq!(output.status.code(), Some(0), "the document resolves: {}", shown(&output));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    assert!(!stdout.contains("s3cret"), "the value stays out of the report: {stdout}");
    assert!(stdout.contains("printoptim"), "the project is named: {stdout}");
    assert!(stdout.contains("${WEBHOOK_URL}"), "the reference is shown: {stdout}");
    assert!(
        stdout.contains("PRINTOPTIM_LOCAL_DATABASE_URL"),
        "an environment is shown as the variable it names: {stdout}"
    );
}
