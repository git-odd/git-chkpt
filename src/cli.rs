use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "git-chkpt", bin_name = "git chkpt")]
#[command(about = "Local per-worktree checkpoints for Git workspaces")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Save the current managed file universe.
    Save {
        /// Optional checkpoint message.
        #[arg(trailing_var_arg = true)]
        message: Vec<String>,
    },
    /// List checkpoints for the current worktree.
    #[command(alias = "ls")]
    List,
    /// Show checkpoint metadata and summary. Defaults to the latest checkpoint.
    Show {
        /// Full checkpoint ID or unique prefix.
        checkpoint: Option<String>,
    },
    /// Compare a checkpoint to the current managed file universe.
    Diff {
        /// Full checkpoint ID or unique prefix. Defaults to the latest checkpoint.
        checkpoint: Option<String>,
    },
    /// Restore the current managed file universe from a checkpoint.
    Restore {
        /// Full checkpoint ID or unique prefix. Defaults to the latest checkpoint.
        checkpoint: Option<String>,
    },
    /// Logically delete checkpoints from public git-chkpt commands.
    #[command(alias = "rm")]
    Delete {
        /// Full checkpoint IDs or unique prefixes.
        checkpoints: Vec<String>,
    },
}

pub fn join_message(parts: Vec<String>) -> Option<String> {
    let message = parts.join(" ").trim().to_owned();
    (!message.is_empty()).then_some(message)
}
