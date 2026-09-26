//! Under `--json`, every command is one envelope.
//!
//! The envelope is what a machine reads instead of learning four output shapes: the mapped
//! exit beside the tool's raw one, the skew that was tolerated, and one payload whose kind is
//! stated rather than guessed.
//!
//! That last part is the whole of D3 and is what most of this file is about. `fraise` decides
//! whether a payload is JSON from **what it asked the tool for**, never from what came back:
//! a tool that prints `{"unions": 94}` as its ordinary text is carrying text, and a face that
//! parsed it because it parses would be a face that guesses. So the two cases here are a stub
//! asked for JSON and a stub not asked, printing the same bytes, and the envelope tells them
//! apart.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The exit `fraise` refuses with, which is confiture's `precondition_failed` written as the
/// integer the frozen contract gives that class.
const REFUSED: i32 = 2;

/// Bytes that are JSON when read as JSON and are text when nobody asked for JSON. Every stub
/// below prints exactly this, so the only thing that can distinguish the envelopes is what
/// `fraise` was told it had asked for.
const LOOKS_LIKE_JSON: &str = r#"{"unions": 94}"#;

/// A case's own directory: the stub programs, and the sentinel each stub writes when it runs.
struct Case {
    root: PathBuf,
}

impl Case {
    fn new(name: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("envelope").join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("bin")).expect("the case directory is created");
        Self { root }
    }

    fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    fn sentinel(&self, program: &str) -> PathBuf {
        self.root.join(format!("{program}.ran"))
    }

    /// A stub that answers `--version` with `version`, and on any other invocation records the
    /// arguments it was given, prints `stdout` and exits `exit`.
    ///
    /// Only shell builtins: `PATH` holds the stub directory alone when these run.
    fn tool(&self, program: &str, version: &str, stdout: &str, exit: i32) {
        self.script(
            program,
            &format!(
                "echo \"args: $*\" >'{sentinel}'\necho '{stdout}'\nexit {exit}\n",
                sentinel = self.sentinel(program).display()
            ),
            version,
        );
    }

    /// A stub that is ended by a signal rather than by exiting, which is the one way a child
    /// comes back with no exit code at all.
    fn signalled_tool(&self, program: &str, version: &str) {
        self.script(program, "kill -TERM $$\n", version);
    }

    fn script(&self, program: &str, body: &str, version: &str) {
        let path = self.bin().join(program);
        let script = format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then\n\
             echo '{version}'\n\
             exit 0\n\
             fi\n\
             {body}"
        );
        fs::write(&path, script).expect("the stub is written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("the stub is made executable");
    }

    /// What the stub recorded, or `None` when it never ran.
    fn ran(&self, program: &str) -> Option<String> {
        fs::read_to_string(self.sentinel(program)).ok()
    }

    /// `fraise`, seeing only this case's stubs.
    fn fraise(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fraise"))
            .args(args)
            .env("PATH", self.bin())
            .current_dir(&self.root)
            .output()
            .expect("the built binary runs")
    }
}

/// The envelope `fraise` wrote, which under `--json` is the whole of its standard output.
fn envelope(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!("`fraise --json` did not emit one document ({error}): {}", shown(output))
    })
}

fn shown(output: &Output) -> String {
    format!(
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The case D3 exists for. The tool printed JSON, and nobody asked it to, so the payload is
/// the text it printed — carried exactly, parsed never.
#[test]
fn text_that_looks_like_json_is_carried_as_text() {
    let case = Case::new("text");
    case.tool("fraiseql", "fraiseql 2.14.1", LOOKS_LIKE_JSON, 2);

    let output = case.fraise(&["--json", "tool", "fraiseql", "compile"]);
    let envelope = envelope(&output);

    assert_eq!(envelope["payload_kind"], "text", "nobody asked for JSON: {envelope}");
    assert_eq!(
        envelope["payload"],
        serde_json::Value::String(format!("{LOOKS_LIKE_JSON}\n")),
        "the payload is the bytes as a string, not a document: {envelope}"
    );
    assert_eq!(envelope["command"], "tool");
    assert_eq!(envelope["tool"], "fraiseql");
}

/// The two exits are two facts, and the envelope carries both: fraiseql's own 2 is a
/// validation failure, which the umbrella's one taxonomy numbers 5.
#[test]
fn the_mapped_exit_and_the_tools_own_exit_both_appear_and_can_differ() {
    let case = Case::new("exits");
    case.tool("fraiseql", "fraiseql 2.14.1", LOOKS_LIKE_JSON, 2);

    let output = case.fraise(&["--json", "tool", "fraiseql", "compile"]);
    let envelope = envelope(&output);

    assert_eq!(envelope["exit"], 5, "the mapped exit: {envelope}");
    assert_eq!(envelope["tool_exit"], 2, "the tool's own: {envelope}");
    assert_eq!(envelope["ok"], false, "a failure is not ok: {envelope}");
    assert_eq!(output.status.code(), Some(5), "and the process exits the mapped one");
}

/// The other half of D3: a payload is a document when `fraise` asked the tool for one. The
/// caller writing the tool's arguments is the only one who knows that, so it is the caller who
/// says — and the arguments still reach the tool exactly as written.
#[test]
fn json_is_a_document_when_fraise_was_told_it_asked_for_one() {
    let case = Case::new("json");
    case.tool("fraiseql", "fraiseql 2.14.1", LOOKS_LIKE_JSON, 0);

    let output = case.fraise(&[
        "--json",
        "tool",
        "--payload",
        "json",
        "fraiseql",
        "compile",
        "--json",
    ]);
    let envelope = envelope(&output);

    assert_eq!(envelope["payload_kind"], "json", "it was asked for: {envelope}");
    assert_eq!(envelope["payload"]["unions"], 94, "the payload is walkable: {envelope}");
    assert_eq!(envelope["ok"], true, "{envelope}");
    assert_eq!(envelope["tool_exit"], 0, "{envelope}");

    let recorded = case.ran("fraiseql").expect("the verb reached the tool");
    assert!(
        recorded.contains("args: compile --json"),
        "the arguments pass through as the caller wrote them: {recorded}"
    );
}

/// `fraise` asked, and the tool did not keep the promise. The bytes are kept and the envelope
/// says what they are — text — because a payload cannot be called JSON on the strength of
/// having been requested.
#[test]
fn a_promise_of_json_the_tool_did_not_keep_is_reported_as_text() {
    let case = Case::new("broken-promise");
    case.tool("fraiseql", "fraiseql 2.14.1", "compiling: 94 unions", 0);

    let output = case.fraise(&["--json", "tool", "--payload", "json", "fraiseql", "compile"]);
    let envelope = envelope(&output);

    assert_eq!(envelope["payload_kind"], "text", "it did not parse: {envelope}");
    assert_eq!(envelope["payload"], "compiling: 94 unions\n", "the bytes are kept: {envelope}");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stderr.contains("fraiseql") && stderr.to_lowercase().contains("json"),
        "the broken promise is said out loud rather than absorbed: {stderr}"
    );
}

/// `doctor` already emits a report, and the envelope carries that report rather than a second
/// shape of it. CI reads the install commands out of it, so this is the shape that must hold.
#[test]
fn doctors_report_is_the_envelopes_payload() {
    let case = Case::new("doctor");
    case.tool("fraiseql", "fraiseql 2.14.1", "", 0);

    let output = case.fraise(&["--json", "doctor"]);
    let envelope = envelope(&output);

    assert_eq!(envelope["command"], "doctor");
    assert_eq!(envelope["tool"], serde_json::Value::Null, "no tool was dispatched to");
    assert_eq!(envelope["tool_exit"], serde_json::Value::Null, "so there is no raw exit");
    assert_eq!(envelope["payload_kind"], "json", "the report is a document: {envelope}");

    let tools = envelope["payload"]["tools"].as_array().expect("the report's findings");
    assert_eq!(tools.len(), 4, "every row of the table is in the payload: {envelope}");
    assert!(
        tools.iter().any(|finding| finding["install"].is_string()),
        "the install commands CI reads are still where it reads them: {envelope}"
    );
}

/// A command that never reached a tool is still one document: the refusal is what `fraise` had
/// to say, so it is the payload, and the raw exit is null because nothing ran to produce one.
#[test]
fn a_refusal_is_an_envelope_too() {
    let case = Case::new("refused");
    case.tool("fraiseql", "fraiseql 2.13.0", LOOKS_LIKE_JSON, 0);

    let output = case.fraise(&["--json", "tool", "fraiseql", "compile"]);
    let envelope = envelope(&output);

    assert_eq!(output.status.code(), Some(REFUSED), "{}", shown(&output));
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["tool"], "fraiseql", "the tool it refused to run is named");
    assert_eq!(envelope["tool_exit"], serde_json::Value::Null, "nothing ran: {envelope}");
    assert_eq!(envelope["payload_kind"], "text");
    let payload = envelope["payload"].as_str().unwrap_or_default();
    assert!(payload.contains("2.13.0") && payload.contains("2.14.1"), "{envelope}");
    assert_eq!(case.ran("fraiseql"), None, "the verb must not have reached the tool");
}

/// A tolerated skew is a field rather than a line someone has to notice, which is what the
/// guard left for this envelope to carry.
#[test]
fn a_tolerated_skew_is_a_field_of_the_envelope() {
    let case = Case::new("tolerated");
    case.tool("fraiseql", "fraiseql 2.13.0", LOOKS_LIKE_JSON, 0);

    let output = case.fraise(&[
        "--json",
        "--allow-version-skew",
        "tool",
        "fraiseql",
        "compile",
    ]);
    let envelope = envelope(&output);

    assert!(output.status.success(), "{}", shown(&output));
    assert_eq!(envelope["tolerated"]["tool"], "fraiseql", "{envelope}");
    assert_eq!(envelope["tolerated"]["found"], "2.13.0", "{envelope}");
    assert!(
        envelope["tolerated"]["allowed"].as_str().unwrap_or_default().contains("2.14.1"),
        "what it was tolerated against: {envelope}"
    );
}

/// A signal leaves no exit code, and the envelope says so rather than inventing one: `tool` is
/// named, so a null `tool_exit` there means the tool ran and never returned a number, and the
/// mapped exit is the shell's own convention for the signal.
#[test]
fn a_tool_ended_by_a_signal_has_no_raw_exit_of_its_own() {
    let case = Case::new("signalled");
    case.signalled_tool("fraiseql", "fraiseql 2.14.1");

    let output = case.fraise(&["--json", "tool", "fraiseql", "compile"]);
    let envelope = envelope(&output);

    assert_eq!(envelope["tool"], "fraiseql", "the tool ran: {envelope}");
    assert_eq!(envelope["tool_exit"], serde_json::Value::Null, "and returned nothing");
    assert_eq!(envelope["exit"], 143, "SIGTERM as the shell numbers it: {envelope}");
    assert_eq!(envelope["ok"], false);
}

/// Without `--json` nothing changes: the tool's output is the tool's, written straight to the
/// terminal as it is produced, and no envelope is wrapped around it. A person watching a
/// migration should not have to wait for it to finish to see it start.
#[test]
fn without_json_the_tools_output_is_its_own() {
    let case = Case::new("streamed");
    case.tool("fraiseql", "fraiseql 2.14.1", LOOKS_LIKE_JSON, 0);

    let output = case.fraise(&["tool", "fraiseql", "compile"]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    assert_eq!(stdout, format!("{LOOKS_LIKE_JSON}\n"), "the tool's own output, whole");
    assert!(!stdout.contains("payload_kind"), "no envelope was wrapped around it: {stdout}");
}
