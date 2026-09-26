//! `fraise` — one command for the FraiseQL stack.
//!
//! The binary is a thin face over this library: the contracts the umbrella holds
//! four tools to live here, so a test can reach them without a process.

pub mod compatibility;
pub mod dispatch;
pub mod doctor;
pub mod envelope;
pub mod exit_table;
pub mod tool_version;
