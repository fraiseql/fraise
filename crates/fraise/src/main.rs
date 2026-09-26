//! `fraise` — one command for the FraiseQL stack.
//!
//! The umbrella owns the face; the four tools underneath keep their contracts.

use clap::Parser;

/// The umbrella command.
#[derive(Parser)]
#[command(name = "fraise", version, about = "One command for the FraiseQL stack")]
struct Cli {}

fn main() {
    let Cli {} = Cli::parse();
}
