//! The command line as one type.
//!
//! Every verb `fraise` grows is declared here, so the face a person or an agent
//! learns has one source in the tree rather than one per subcommand module.

use clap::Parser;

/// The umbrella command.
///
/// It carries no options yet: the version it prints is rendered by the parser
/// from the manifest, which is the whole of this cycle's contract.
#[derive(Debug, Parser)]
#[command(name = "fraise", version, about = "One command for the FraiseQL stack")]
pub struct Cli {}
