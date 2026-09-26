//! `fraise` — one command for the FraiseQL stack.
//!
//! The umbrella owns the face; the four tools underneath keep their contracts.

mod cli;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use fraise::compatibility::CompatibilityTable;
use fraise::dispatch::{Dispatcher, Tolerance};
use fraise::doctor;

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let table = CompatibilityTable::vendored();

    match &cli.command {
        Command::Doctor => {
            let report = doctor::examine(table);
            if cli.json {
                print!("{}", report.to_json());
            } else {
                print!("{}", report.render());
            }
            exit(report.exit())
        },
        Command::Tool { tool, args } => dispatch_tool(table, &cli, tool, args),
    }
}

/// Hand a verb to one of the tools, with the guard in front of it.
///
/// Every exit here is the compatibility table's refusal class but one: what the tool itself
/// came to, in the umbrella's taxonomy.
fn dispatch_tool(
    table: &'static CompatibilityTable,
    cli: &Cli,
    tool: &str,
    args: &[String],
) -> ExitCode {
    let refused = table.refusal_class().exit();
    let directory = match working_directory(cli.directory.clone()) {
        Ok(directory) => directory,
        Err(problem) => {
            eprintln!("{problem}");
            return exit(refused);
        },
    };
    let tolerance = if cli.allow_version_skew {
        Tolerance::TolerateSkew
    } else {
        Tolerance::Refuse
    };
    let dispatcher = Dispatcher::new(table, directory, tolerance);

    let cleared = match dispatcher.clear(tool) {
        Ok(cleared) => cleared,
        Err(refusal) => {
            eprintln!("{}", refusal.message());
            return exit(refusal.exit());
        },
    };
    // Said before the verb runs, so it is on the terminal even if the tool then hangs or floods
    // it. Cycle 5's envelope records it as a field.
    if let Some(skew) = cleared.tolerated() {
        eprintln!("{skew}");
    }
    match dispatcher.run(cleared, args) {
        Ok(outcome) => exit(outcome.exit()),
        Err(error) => {
            eprintln!("fraise could not run {tool}: {error}");
            exit(refused)
        },
    }
}

/// The directory the tools will run in, made absolute here so that what is passed to a child is
/// a decision rather than whatever the process happened to be in.
fn working_directory(asked: Option<PathBuf>) -> Result<PathBuf, String> {
    let directory = match asked {
        Some(directory) => directory,
        None => std::env::current_dir()
            .map_err(|error| format!("fraise cannot tell which directory it is in: {error}"))?,
    };
    directory
        .canonicalize()
        .map_err(|error| format!("fraise cannot run the tools in {}: {error}", directory.display()))
}

/// An exit of the umbrella's one taxonomy as the process returns it. The classes are
/// confiture's, which the loaded contract holds to `0..=8`, so the fallback is unreachable
/// rather than a choice about what an out-of-range exit would mean.
fn exit(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}
