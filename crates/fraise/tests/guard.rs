//! No tool is invoked without its version being checked first.
//!
//! `doctor` reports the compatibility table; this is the table in force on the path that
//! matters, where a verb is about to be handed to a tool. The refusal has to happen *before*
//! the exec, so every stub here records the fact that it ran — a test that only asserted on
//! the exit could not tell a refusal from a tool that ran and failed.
//!
//! The stubs also record the directory they ran in. fraiseql#1387 is `compile` reading
//! `fraiseql.toml` from the working directory: run from a sibling directory it emitted 0
//! unions of 94 at exit 0, silently. A dispatcher that leaves the child's directory to chance
//! inherits that failure, so the child's directory is asserted rather than assumed.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The exit `fraise` refuses with, which is confiture's `precondition_failed` written as the
/// integer the frozen contract gives that class.
const REFUSED: i32 = 2;

/// A case's own directory: the stub programs, the sentinel each stub writes when it runs, and
/// a project directory to dispatch into.
struct Case {
    root: PathBuf,
}

impl Case {
    fn new(name: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("guard").join(name);
        let _ = fs::remove_dir_all(&root);
        for directory in ["bin", "project"] {
            fs::create_dir_all(root.join(directory)).expect("the case directory is created");
        }
        Self { root }
    }

    fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    fn project(&self) -> PathBuf {
        self.root.join("project")
    }

    fn sentinel(&self, program: &str) -> PathBuf {
        self.root.join(format!("{program}.ran"))
    }

    /// A stub that answers `--version` with `version_line`, and on any other invocation records
    /// where it ran and with what, then exits `exit`.
    ///
    /// Only shell builtins: `PATH` holds the stub directory alone when these run, so a stub
    /// that shelled out to `cat` would exit 127 instead of doing its job.
    fn tool(&self, program: &str, version_line: &str, exit: i32) {
        let path = self.bin().join(program);
        let sentinel = self.sentinel(program);
        let script = format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then\n\
             echo '{version_line}'\n\
             exit 0\n\
             fi\n\
             {{ echo \"cwd: $(pwd -P)\"; echo \"args: $*\"; }} >'{sentinel}'\n\
             exit {exit}\n",
            sentinel = sentinel.display()
        );
        fs::write(&path, script).expect("the stub is written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("the stub is made executable");
    }

    /// What the stub recorded, or `None` when it never ran.
    fn ran(&self, program: &str) -> Option<String> {
        fs::read_to_string(self.sentinel(program)).ok()
    }

    /// `fraise`, seeing only this case's stubs, dispatching into this case's project directory.
    ///
    /// The environment holds `PATH` and nothing else. A dispatch now resolves which database it is
    /// about, and `FRAISE_ENVIRONMENT` or a DSN variable in the environment of whoever runs the
    /// suite would decide that for it — one ambient `FRAISER_*` reddened seven unrelated tests in
    /// fraisier-core (#64), and these cases are about versions rather than about databases.
    fn fraise(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fraise"))
            .args(args)
            .env_clear()
            .env("PATH", self.bin())
            .current_dir(&self.root)
            .output()
            .expect("the built binary runs")
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

/// The guard's whole point: a version the table does not allow stops the dispatch before the
/// tool is executed at all.
#[test]
fn a_version_outside_the_table_is_refused_before_the_verb_runs() {
    let case = Case::new("outside");
    case.tool("fraiseql", "fraiseql 2.13.0", 0);

    let output = case.fraise(&["tool", "fraiseql", "compile"]);
    assert_eq!(
        output.status.code(),
        Some(REFUSED),
        "an unallowed version is a failed precondition: {}",
        shown(&output)
    );
    assert_eq!(case.ran("fraiseql"), None, "the verb must not have reached the tool");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(stderr.contains("2.13.0"), "the refusal names what was found: {stderr}");
    assert!(stderr.contains("2.14.1"), "the refusal names what was allowed: {stderr}");
    assert!(
        stderr.contains("--allow-version-skew"),
        "the refusal names its escape hatch: {stderr}"
    );
}

/// The escape hatch, both ways of asking for it. A tolerated skew is not a silent one: what
/// was tolerated is said, because the next person to read a strange result needs to know the
/// versions did not match.
#[test]
fn a_tolerated_skew_runs_the_verb_and_says_what_it_tolerated() {
    let case = Case::new("tolerated");
    case.tool("fraiseql", "fraiseql 2.13.0", 0);

    let output = case.fraise(&["--allow-version-skew", "tool", "fraiseql", "compile"]);
    assert!(output.status.success(), "a tolerated skew proceeds: {}", shown(&output));
    let recorded = case.ran("fraiseql").expect("the verb reached the tool");
    assert!(recorded.contains("args: compile"), "the tool was given the verb: {recorded}");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(stderr.contains("2.13.0"), "the tolerated version is named: {stderr}");
    assert!(stderr.contains("2.14.1"), "what it was tolerated against is named: {stderr}");

    let by_environment = Command::new(env!("CARGO_BIN_EXE_fraise"))
        .args(["tool", "fraiseql", "compile"])
        .env_clear()
        .env("PATH", case.bin())
        .env("FRAISE_ALLOW_VERSION_SKEW", "1")
        .current_dir(case.project())
        .output()
        .expect("the built binary runs");
    assert!(
        by_environment.status.success(),
        "the environment variable tolerates it too: {}",
        shown(&by_environment)
    );
}

/// The child's working directory is chosen, not inherited by accident: fraiseql#1387 reads
/// `fraiseql.toml` from it, and a tool run in the wrong directory succeeds while doing nothing.
#[test]
fn the_child_runs_in_the_directory_it_was_given() {
    let case = Case::new("directory");
    case.tool("confiture", "confiture version 1.19.0", 0);

    let output = case.fraise(&[
        "--directory",
        "project",
        "tool",
        "confiture",
        "migrate",
        "status",
    ]);
    assert!(output.status.success(), "the dispatch proceeds: {}", shown(&output));

    let recorded = case.ran("confiture").expect("the verb reached the tool");
    let project = case.project().canonicalize().expect("the project directory exists");
    assert!(
        recorded.contains(&format!("cwd: {}", project.display())),
        "the tool ran in {}, and recorded {recorded}",
        project.display()
    );
    assert!(
        recorded.contains("args: migrate status"),
        "the arguments pass through: {recorded}"
    );
}

/// The tools the umbrella speaks for are the table's rows. An unknown one is refused with the
/// names it does know, rather than attempted.
#[test]
fn a_tool_the_table_does_not_name_is_refused_with_the_ones_it_does() {
    let case = Case::new("unknown");
    case.tool("psql", "psql (PostgreSQL) 18.1", 0);

    let output = case.fraise(&["tool", "psql", "--version"]);
    assert_eq!(
        output.status.code(),
        Some(REFUSED),
        "an unknown tool is refused: {}",
        shown(&output)
    );
    assert_eq!(case.ran("psql"), None, "an unknown tool is not executed");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    for known in ["confiture", "fraiseql", "fraisier", "specql"] {
        assert!(stderr.contains(known), "the refusal names {known}: {stderr}");
    }
}

/// specql has no release for the table to require, so the guard has nothing to check a build
/// against: absent it is refused as not installed, and present it is a version nothing vouches
/// for, which the escape hatch is the only way past.
#[test]
fn a_tool_with_no_pinned_release_needs_the_escape_hatch_even_when_it_is_installed() {
    let case = Case::new("unvouched");

    let absent = case.fraise(&["tool", "specql", "generate"]);
    assert_eq!(absent.status.code(), Some(REFUSED), "nothing to run: {}", shown(&absent));

    // The hatch tolerates a version the table disagrees with. A tool that is not there is not
    // a disagreement, so it stays refused however loudly the caller asks.
    let absent_but_tolerant = case.fraise(&["--allow-version-skew", "tool", "specql", "generate"]);
    assert_eq!(
        absent_but_tolerant.status.code(),
        Some(REFUSED),
        "there is nothing to tolerate: {}",
        shown(&absent_but_tolerant)
    );

    case.tool("specql", "specql 2.0.0", 0);
    let installed = case.fraise(&["tool", "specql", "generate"]);
    assert_eq!(
        installed.status.code(),
        Some(REFUSED),
        "a build no release vouches for is refused: {}",
        shown(&installed)
    );
    assert_eq!(case.ran("specql"), None, "it was refused before it ran");

    let tolerated = case.fraise(&["--allow-version-skew", "tool", "specql", "generate"]);
    assert!(tolerated.status.success(), "the hatch is the way past: {}", shown(&tolerated));
    assert!(case.ran("specql").is_some(), "and then the verb runs");
}

/// The umbrella has one exit taxonomy, so a tool's own exit arrives mapped: fraiseql's 2 is
/// `validation_failed`, which is confiture's 5. The raw exit has its own field in the
/// envelope; the process exit is the mapped one.
#[test]
fn the_tools_exit_arrives_mapped_through_the_contract() {
    let case = Case::new("mapped");
    case.tool("fraiseql", "fraiseql 2.14.1", 2);

    let output = case.fraise(&["tool", "fraiseql", "compile"]);
    assert!(case.ran("fraiseql").is_some(), "the verb ran: {}", shown(&output));
    assert_eq!(
        output.status.code(),
        Some(5),
        "fraiseql's 2 is a validation failure, which the contract numbers 5: {}",
        shown(&output)
    );
}

/// A version is read once per process, whatever asks for it. A command that crosses several
/// boundaries checks each one, and paying a process launch per check — or worse, holding two
/// answers about the same tool inside one command — is what the cache exists to prevent.
///
/// Counted by a stub that appends a line every time it runs, reached by its absolute path so
/// that nothing here depends on `PATH`.
#[test]
fn a_tools_version_is_read_once_per_process() {
    let case = Case::new("cached");
    let counted = case.bin().join("counted");
    let log = case.root.join("counted.log");
    fs::write(
        &counted,
        format!("#!/bin/sh\necho ran >>'{}'\necho 'counted 1.2.3'\n", log.display()),
    )
    .expect("the stub is written");
    fs::set_permissions(&counted, fs::Permissions::from_mode(0o755))
        .expect("the stub is made executable");

    let program = counted.to_str().expect("the path is utf-8");
    let first = fraise::tool_version::read(program);
    let second = fraise::tool_version::read(program);

    assert_eq!(first, second, "the same answer both times");
    assert!(matches!(first, fraise::tool_version::Reading::Version(_)), "{first:?}");
    let runs = fs::read_to_string(&log).expect("the stub ran at least once");
    assert_eq!(runs.lines().count(), 1, "it was executed once, and recorded: {runs:?}");
}
