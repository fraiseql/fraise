//! `fraise` — one command for the FraiseQL stack.
//!
//! The umbrella owns the face; the four tools underneath keep their contracts.

mod cli;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use fraise::compatibility::CompatibilityTable;
use fraise::config::{Config, Problem};
use fraise::dispatch::{Dispatcher, Tolerance};
use fraise::doctor;
use fraise::dsn::{self, Declared, Flags, Handover};
use fraise::envelope::{Asked, Envelope, Payload, PayloadKind};
use fraise::interpolation::Process;

use crate::cli::{Cli, Command, ConfigCommand, PayloadMode};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let table = CompatibilityTable::vendored();

    match &cli.command {
        Command::Config {
            what: ConfigCommand::Show,
        } => show_config(table, &cli),
        Command::Doctor => examine(table, &cli),
        Command::Tool {
            mutating,
            payload,
            tool,
            args,
        } => dispatch_tool(table, &cli, *mutating, *payload, tool, args),
    }
}

/// Read `fraise.toml` and say what it holds.
///
/// The reading is the work: a document that cannot be acted on is refused here rather than by
/// the first verb that needed it. What is shown is the document as written — a value that came
/// from the environment appears as its `${VAR}` reference — so the report is the same for a
/// person and for a machine and neither carries a secret.
fn show_config(table: &'static CompatibilityTable, cli: &Cli) -> ExitCode {
    let command = cli.command.name();
    let directory = match working_directory(cli.directory.clone()) {
        Ok(directory) => directory,
        // A directory that cannot be resolved is not a fault of the file: nothing has been read
        // yet, and it is the same unmet precondition a dispatch would refuse with.
        Err(problem) => return report_problem(cli, command, table.refusal_class().exit(), problem),
    };
    let loaded = match Config::at(&directory).and_then(|config| config.resolve(&Process)) {
        Ok(loaded) => loaded,
        Err(problem) => {
            return report_problem(cli, command, problem.exit(), problem.message().to_owned());
        },
    };
    // Which database the document says a command would be about, reported rather than acted on:
    // nothing here runs a tool, so no DSN is read. A variable the document names and the machine
    // does not set is a fact the report carries, not a refusal — that one belongs where the DSN
    // is needed. An ambiguity is refused, because an invocation nobody could resolve is not a
    // document anyone can act on.
    let database = match dsn::resolve(flags(cli, false), loaded.declared(), &Process) {
        Ok(database) => database,
        Err(problem) => {
            return report_problem(cli, command, problem.exit(), problem.message().to_owned());
        },
    };
    if cli.json {
        let payload = loaded.payload(&database);
        print!("{}", Envelope::answered(command, 0, &payload).to_json());
    } else {
        print!("{}", loaded.render(&database));
    }
    ExitCode::SUCCESS
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
    mutating: bool,
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

    // Asked after the guard, because whether this face speaks for a tool at all comes before
    // which database it would be about — and answered before the exec, so a command that has no
    // source it may use never reaches the tool.
    let handover = match hand_over(cli, dispatcher.directory(), mutating) {
        Ok(handover) => handover,
        Err(problem) => {
            return report_refusal(
                cli,
                command,
                tool,
                problem.exit(),
                problem.message().to_owned(),
            );
        },
    };

    match dispatcher.run(cleared, args, asked, &handover) {
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

/// What this invocation says about where its DSN comes from.
fn flags(cli: &Cli, mutating: bool) -> Flags<'_> {
    Flags {
        environment: cli.environment.as_deref(),
        variable: cli.database_url_env.as_deref(),
        mutating,
    }
}

/// The DSN this command runs against, under the names the stack reads it by.
///
/// The document is read here rather than at the top of the command: `fraise tool` reaches a tool
/// whether or not the directory is a project — that is what makes it the fallback this face
/// promises — but a document that is there is read, and read whole, because a command about to
/// touch a database must not be the one that ignored the file saying which database.
fn hand_over(cli: &Cli, directory: &Path, mutating: bool) -> Result<Handover, Problem> {
    let config = Config::find(directory)?;
    let declared = config.as_ref().map_or(Declared::NoDocument, Config::declared);
    dsn::resolve(flags(cli, mutating), declared, &Process)?.handover(&Process)
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

/// Say why a command `fraise` answers out of its own files could not be answered: to a person on
/// standard error always, and to a machine as the payload of the envelope that command is owed.
///
/// No tool is named, because none was reached — and for these commands, none would have been.
fn report_problem(cli: &Cli, command: &str, exit_code: i32, message: String) -> ExitCode {
    eprintln!("{message}");
    if cli.json {
        let payload = Payload::Text(message);
        print!("{}", Envelope::answered(command, exit_code, &payload).to_json());
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
