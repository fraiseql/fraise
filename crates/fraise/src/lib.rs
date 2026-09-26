//! `fraise` — one command for the FraiseQL stack.
//!
//! The binary is a thin face over this library: the contracts the umbrella holds
//! four tools to live here, so a test can reach them without a process.

pub mod compatibility;
pub mod config;
pub mod dispatch;
pub mod doctor;
pub mod envelope;
pub mod exit_table;
pub mod interpolation;
pub mod tool_version;

// Reason: the source of this crate, read by the two tests whose claim is about the shape of
// the tree rather than about a value. It ships nothing.
#[cfg(test)]
mod source;
