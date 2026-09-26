//! `fraise` — one command for the FraiseQL stack.
//!
//! The umbrella owns the face; the four tools underneath keep their contracts.

mod cli;

use std::process::ExitCode;

use clap::Parser;
use fraise::compatibility::CompatibilityTable;
use fraise::doctor;

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => {
            let report = doctor::examine(CompatibilityTable::vendored());
            if cli.json {
                print!("{}", report.to_json());
            } else {
                print!("{}", report.render());
            }
            exit(report.exit())
        },
    }
}

/// An exit of the umbrella's one taxonomy as the process returns it. The classes are
/// confiture's, which the loaded contract holds to `0..=8`, so the fallback is unreachable
/// rather than a choice about what an out-of-range exit would mean.
fn exit(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}
