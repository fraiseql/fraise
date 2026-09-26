//! The command line as one type.
//!
//! Every verb `fraise` grows is declared here, so the face a person or an agent
//! learns has one source in the tree rather than one per subcommand module.

use clap::{Parser, Subcommand};

/// The umbrella command.
#[derive(Debug, Parser)]
#[command(
    name = "fraise",
    version,
    about = "One command for the FraiseQL stack",
    arg_required_else_help = true
)]
pub struct Cli {
    /// Report as JSON instead of as text.
    ///
    /// Global from the start because it is the same promise for every verb: the machine
    /// reading `fraise` gets a document rather than a shape it has to recognise. Cycle 5
    /// makes that document one envelope; today it is the command's own report.
    #[arg(long, global = true)]
    pub json: bool,

    /// The verb to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The verbs.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Measure the tools on this machine against the compatibility table.
    Doctor,
}
