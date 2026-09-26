//! The command line as one type.
//!
//! Every verb `fraise` grows is declared here, so the face a person or an agent
//! learns has one source in the tree rather than one per subcommand module.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

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
    /// reading `fraise` gets one envelope rather than a shape it has to recognise per
    /// command.
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
    /// Read `fraise.toml`, the one file a project author writes.
    Config {
        /// What to do with it.
        #[command(subcommand)]
        what: ConfigCommand,
    },

    /// Measure the tools on this machine against the compatibility table.
    Doctor,

    /// Run one of the stack's tools through the umbrella, version-guarded.
    ///
    /// The fallback the face promises: whatever the verbs do not cover yet, the tool itself
    /// can still be reached, with the version checked and the exit mapped.
    Tool {
        /// What the tool's standard output is, when `--json` makes an envelope of it.
        ///
        /// `fraise` never decides this by looking at the output: text that looks like JSON
        /// is text. Only the caller that wrote the tool's arguments knows whether they asked
        /// it for a document, so the caller is who says.
        ///
        /// It needs `--json`, which is enforced where it is read rather than by clap's
        /// `requires`: a global given before the subcommand — `fraise --json tool …`, the
        /// spelling this face documents — is not visible to the subcommand's own validation
        /// (measured, clap 4).
        #[arg(long, value_name = "KIND")]
        payload: Option<PayloadMode>,

        /// The tool, as the compatibility table names it.
        tool: String,

        /// What to pass it.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// What `fraise config` can do with `fraise.toml`.
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Read it, refuse it if it cannot be acted on, and show what it says.
    ///
    /// Values that came from the environment are shown as the `${VAR}` references the file
    /// holds rather than as what they resolved to, so this is safe to paste and safe to log.
    Show,
}

impl Command {
    /// The verb as the face spells it, which is what the envelope reports as `command`.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Config {
                what: ConfigCommand::Show,
            } => "config show",
            Self::Doctor => "doctor",
            Self::Tool { .. } => "tool",
        }
    }
}

/// What a tool was asked for, as the face lets a caller say it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PayloadMode {
    /// Bytes, carried exactly as the tool wrote them. The default, because it assumes
    /// nothing about a tool's output.
    Text,
    /// A document, because the arguments given to the tool asked it for one.
    Json,
}
