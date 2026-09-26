//! `fraise` — one command for the FraiseQL stack.
//!
//! The umbrella owns the face; the four tools underneath keep their contracts.

mod cli;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use fraise::compatibility::CompatibilityTable;
use fraise::dispatch::{Dispatcher, Tolerance};
use fraise::doctor;
use fraise::envelope::{Asked, Envelope, Payload, PayloadKind};

use crate::cli::{Cli, Command, PayloadMode};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let table = CompatibilityTable::vendored();

    match &cli.command {
        Command::Doctor => examine(table, &cli),
        Command::Tool {
            payload,
            tool,
            args,
        } => dispatch_tool(table, &cli, *payload, tool, args),
    }
}

/// Measure this machine against the compatibility table.
///
/// The report is one thing, said twice: a person reads the rendered lines, a machine reads the
/// same findings as the envelope's payload.
fn examine(table: &'static CompatibilityTable, cli: &Cli) -> ExitCode {
    let report = doctor::examine(table);
    if cli.json {
        let payload = report.payload();
        print!("{}", Envelope::answered(cli.command.name(), report.exit(), &payload).to_json());
    } else {
        print!("{}", report.render());
    }
    exit(report.exit())
}

/// Hand a verb to one of the tools, with the guard in front of it.
///
/// Every exit here is the compatibility table's refusal class but one: what the tool itself
/// came to, in the umbrella's taxonomy.
fn dispatch_tool(
    table: &'static CompatibilityTable,
    cli: &Cli,
    payload: Option<PayloadMode>,
    tool: &str,
    args: &[String],
) -> ExitCode {
    let command = cli.command.name();
    let asked = asked_of(cli, payload);
    let refused = table.refusal_class().exit();
    let directory = match working_directory(cli.directory.clone()) {
        Ok(directory) => directory,
        Err(problem) => return report_refusal(cli, command, tool, refused, problem),
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
            return report_refusal(
                cli,
                command,
                tool,
                refusal.exit(),
                refusal.message().to_owned(),
            );
        },
    };
    // Said before the verb runs, so it is on the terminal even if the tool then hangs or floods
    // it. The envelope records it as a field as well, for a reader that is not watching.
    if let Some(skew) = cleared.tolerated() {
        eprintln!("{skew}");
    }

    match dispatcher.run(cleared, args, asked) {
        Ok(outcome) => {
            if cli.json {
                if asked == Asked::Json && outcome.payload().kind() != PayloadKind::Json {
                    eprintln!(
                        "fraise asked {tool} for JSON and what it wrote is not JSON: the payload \
                         is carried as text rather than guessed at"
                    );
                }
                print!("{}", Envelope::dispatched(command, &outcome).to_json());
            }
            exit(outcome.exit())
        },
        Err(error) => report_refusal(
            cli,
            command,
            tool,
            refused,
            format!("fraise could not run {tool}: {error}"),
        ),
    }
}

/// What `fraise` is asking the tool for on this invocation.
///
/// Without an envelope to fill there is nothing to capture, so the child keeps `fraise`'s own
/// streams and a long-running tool goes on being watchable. With one, the caller's `--payload`
/// says how to read what it wrote, and text is what is assumed of a tool that was told nothing.
///
/// Asking how to read a payload without asking for an envelope is a usage error, and is
/// answered as one, before anything has run.
fn asked_of(cli: &Cli, payload: Option<PayloadMode>) -> Asked {
    match (cli.json, payload) {
        (false, None) => Asked::Nothing,
        (false, Some(_)) => Cli::command()
            .error(
                clap::error::ErrorKind::MissingRequiredArgument,
                "--payload says how to read a tool's output into an envelope, and --json is \
                 what makes one",
            )
            .exit(),
        (true, None | Some(PayloadMode::Text)) => Asked::Text,
        (true, Some(PayloadMode::Json)) => Asked::Json,
    }
}

/// Say why nothing ran: to a person on standard error always, and to a machine as the payload
/// of the envelope it is owed for this command.
fn report_refusal(
    cli: &Cli,
    command: &str,
    tool: &str,
    exit_code: i32,
    message: String,
) -> ExitCode {
    eprintln!("{message}");
    if cli.json {
        let payload = Payload::Text(message);
        print!("{}", Envelope::refused(command, tool, exit_code, &payload).to_json());
    }
    exit(exit_code)
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
