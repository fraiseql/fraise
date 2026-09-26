//! Repository automation. `cargo xtask ci` runs the same gate as CI: the shell
//! scripts under `tools/`, then a format check, clippy with warnings denied, and
//! the test suite.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

fn main() -> ExitCode {
    let task = env::args().nth(1);
    if task.as_deref() == Some("ci") {
        ci()
    } else {
        eprintln!("usage: cargo xtask ci");
        ExitCode::from(2)
    }
}

fn ci() -> ExitCode {
    if !shellcheck() {
        return ExitCode::FAILURE;
    }
    let gates: &[&[&str]] = &[
        &["fmt", "--all", "--check"],
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    ];
    for args in gates {
        if !cargo(args) {
            return ExitCode::FAILURE;
        }
    }
    let tests: &[&str] = if has_nextest() {
        &["nextest", "run", "--workspace", "--all-features"]
    } else {
        &["test", "--workspace", "--all-features"]
    };
    if cargo(tests) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Lint every shell script under `tools/`.
///
/// `tools/package.sh` decides what a release tarball is, and until this ran it was the one file
/// in the tree no gate read. A separate command to remember is how a linter stops being run, so
/// it is part of this one — and the scripts are found rather than named, so the next one is
/// linted without anyone remembering to add it. A `tools/` that has stopped holding scripts
/// fails here instead of quietly linting nothing.
///
/// Every severity, and no version pin. Which findings exist does depend on the shellcheck that
/// runs — the runner image carries 0.9.0 and this machine 0.11.0, and the first thing this gate
/// caught was a line only 0.9.0 objected to (`SC2015`, `A && B || C`) — so the scripts are kept
/// clean under both rather than held to a severity floor that would silence a class of finding
/// for good. A future release adding an info-level check can still redden an untouched tree;
/// that is one commit and usually advice worth taking, which is not the same kind of problem as
/// a formatter whose output depends on its toolchain.
fn shellcheck() -> bool {
    let scripts = scripts();
    if scripts.is_empty() {
        eprintln!("no shell scripts found under tools/, so this gate is measuring nothing");
        return false;
    }
    let arguments: Vec<&str> = scripts.iter().filter_map(|path| path.to_str()).collect();
    if arguments.len() != scripts.len() {
        eprintln!("a script's path is not utf-8, so it cannot be handed to shellcheck");
        return false;
    }
    run("shellcheck", &arguments)
}

/// The shell scripts under `tools/`, in a stable order.
fn scripts() -> Vec<PathBuf> {
    let tools = root().join("tools");
    let Ok(entries) = fs::read_dir(&tools) else {
        eprintln!("{} cannot be read", tools.display());
        return Vec::new();
    };
    let mut scripts: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "sh"))
        .collect();
    scripts.sort();
    scripts
}

/// The repository root, from this crate rather than from the directory `cargo` was run in.
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("this crate is in the workspace")
}

fn has_nextest() -> bool {
    Command::new("cargo")
        .args(["nextest", "--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn cargo(args: &[&str]) -> bool {
    run("cargo", args)
}

/// Run one gate, saying why it could not run when that is the answer — a tool that is not
/// installed and a tool that found something are different failures, and a gate that reports
/// them the same way is one people learn to rerun rather than read.
fn run(program: &str, args: &[&str]) -> bool {
    eprintln!("$ {program} {}", args.join(" "));
    match Command::new(program).args(args).status() {
        Ok(status) => status.success(),
        Err(error) => {
            eprintln!("{program} could not be run: {error}");
            false
        },
    }
}
