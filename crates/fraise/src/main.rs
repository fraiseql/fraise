//! `fraise` — one command for the FraiseQL stack.
//!
//! The umbrella owns the face; the four tools underneath keep their contracts.

mod cli;

use clap::Parser;

use crate::cli::Cli;

fn main() {
    let Cli {} = Cli::parse();
}
