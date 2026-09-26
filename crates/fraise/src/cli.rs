//! The command line as one type.
//!
//! Every verb `fraise` grows is declared here, so the face a person or an agent
//! learns has one source in the tree rather than one per subcommand module.

use std::path::PathBuf;

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

    /// The directory the tools run in. Defaults to the current one.
    ///
    /// Passed to every child explicitly rather than inherited, because a tool that reads its
    /// configuration from the working directory — fraiseql's `compile` does — must read the
    /// one that was meant.
    #[arg(long, short = 'C', global = true, value_name = "PATH")]
    pub directory: Option<PathBuf>,

    /// Run a tool whose version the compatibility table does not allow, reporting the skew.
    ///
    /// The environment variable takes the spellings a script reaches for — `1`, `yes`, `on`,
    /// `true` — because a switch that rejects `1` with a usage error is a switch that reads as
    /// broken.
    #[arg(
        long,
        global = true,
        env = "FRAISE_ALLOW_VERSION_SKEW",
        action = clap::ArgAction::SetTrue,
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    pub allow_version_skew: bool,

    /// The verb to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The verbs.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Measure the tools on this machine against the compatibility table.
    Doctor,

    /// Run one of the stack's tools through the umbrella, version-guarded.
    ///
    /// The fallback the face promises: whatever the verbs do not cover yet, the tool itself
    /// can still be reached, with the version checked and the exit mapped.
    Tool {
        /// The tool, as the compatibility table names it.
        tool: String,

        /// What to pass it.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}
