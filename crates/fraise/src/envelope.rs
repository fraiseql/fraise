//! The one document a machine reads.
//!
//! Four tools report four ways. Under `--json` the umbrella answers in one shape for every
//! command, so a caller learns the shape once: what `fraise` was asked to do, which tool it
//! crossed into, what that came to in the umbrella's taxonomy **and** in the tool's own
//! numbering, what skew was tolerated to get there, and one payload.
//!
//! The payload is where the care is. [`PayloadKind`] is decided by what `fraise` asked the
//! tool for — [`Asked`] — and never by what came back. Text that looks like JSON is text
//! (D3): a face that parsed a payload because it happened to parse would be a face that
//! guesses, and the day a tool prints a JSON-shaped error line to standard output, every
//! caller downstream would be reading it as the answer. So `fraise` only reads a payload as a
//! document when the invocation it ran asked for one, and even then only when it parses —
//! being asked for JSON is not a reason to call something JSON.

use serde::Serialize;

use crate::dispatch::{Outcome, Skew};

/// What `fraise` asked the tool to emit, which decides both how the child's streams are wired
/// and how its answer is read.
///
/// It is a statement about the invocation `fraise` built, never an inference from the bytes
/// that came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// Nothing. The child writes to `fraise`'s own streams as it goes, so a tool that streams
    /// progress to a terminal still does, and there is no payload to carry.
    Nothing,
    /// Text: the child's standard output is captured and carried exactly as it arrived.
    Text,
    /// JSON: the child's standard output is captured and read as a document, because the
    /// arguments handed to the tool asked it for one.
    Json,
}

/// What the payload in this envelope is, so that a reader never has to sniff it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadKind {
    /// There is no payload: the command's output went straight to the terminal rather than
    /// through `fraise`.
    None,
    /// Bytes, as a JSON string. Whatever they look like, nothing parsed them.
    Text,
    /// A document, which `fraise` asked for and which parsed.
    Json,
}

/// What `fraise` has to say about the command: a tool's captured output, `fraise`'s own
/// report, or — when nothing ran — the refusal.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Payload {
    /// Nothing was captured.
    None,
    /// Bytes, exactly as they arrived, lossily decoded so that a tool writing something other
    /// than UTF-8 costs a mangled character rather than the whole answer.
    Text(String),
    /// A document.
    Json(serde_json::Value),
}

impl Payload {
    /// What this payload is, which is the envelope's `payload_kind`.
    #[must_use]
    pub const fn kind(&self) -> PayloadKind {
        match self {
            Self::None => PayloadKind::None,
            Self::Text(_) => PayloadKind::Text,
            Self::Json(_) => PayloadKind::Json,
        }
    }

    /// What a child's captured output amounts to, given what it was asked for.
    ///
    /// Asked for JSON and answering with something else is the tool's broken promise, not a
    /// licence to guess: the bytes are kept as text and the envelope says text, which is the
    /// one honest thing to call a payload nobody could parse.
    pub(crate) fn captured(asked: Asked, output: &[u8]) -> Self {
        match asked {
            Asked::Nothing => Self::None,
            Asked::Text => Self::text(output),
            Asked::Json => {
                serde_json::from_slice(output).map_or_else(|_| Self::text(output), Self::Json)
            },
        }
    }

    fn text(output: &[u8]) -> Self {
        Self::Text(String::from_utf8_lossy(output).into_owned())
    }
}

/// One command's answer, whole.
///
/// Every field is what it means rather than what it holds:
///
/// - `ok` — the command did what it was asked. It is the mapped `exit` being the contract's
///   success, so it never disagrees with the number the process returned.
/// - `command` — the verb of `fraise` that ran, as the face spells it.
/// - `tool` — the tool this command crossed into, or null when it crossed into none. It is
///   also what tells a null `tool_exit` apart: named here, the tool ran.
/// - `exit` — what `fraise` itself exited with, in the umbrella's one taxonomy.
/// - `tool_exit` — the tool's own number, unmapped and un-interpreted. Null when no tool ran,
///   and null when a signal ended one, because a process ended by a signal never returned a
///   number to report; `exit` then carries the shell's own `128 + n` for it.
/// - `tolerated` — the version skew this command was told to let through, or null. A skew is
///   never silent: it is here as a field and on standard error as a line.
/// - `payload_kind` — what the payload is, decided by what `fraise` asked for.
/// - `payload` — what `fraise` has to say: the tool's captured output, `fraise`'s own report,
///   or the refusal that stopped the command.
#[derive(Debug, Serialize)]
pub struct Envelope<'a> {
    ok: bool,
    command: &'a str,
    tool: Option<&'a str>,
    exit: i32,
    tool_exit: Option<i32>,
    tolerated: Option<&'a Skew>,
    payload_kind: PayloadKind,
    payload: &'a Payload,
}

impl<'a> Envelope<'a> {
    /// The envelope for a command `fraise` answered out of its own tables. No tool was run, so
    /// there is no raw exit to put beside the mapped one.
    #[must_use]
    pub const fn answered(command: &'a str, exit: i32, payload: &'a Payload) -> Self {
        Self {
            ok: succeeded(exit),
            command,
            tool: None,
            exit,
            tool_exit: None,
            tolerated: None,
            payload_kind: payload.kind(),
            payload,
        }
    }

    /// The envelope for a command that crossed into a tool and came back.
    #[must_use]
    pub const fn dispatched(command: &'a str, outcome: &'a Outcome<'a>) -> Self {
        Self {
            ok: succeeded(outcome.exit()),
            command,
            tool: Some(outcome.tool()),
            exit: outcome.exit(),
            tool_exit: outcome.tool_exit(),
            tolerated: outcome.tolerated(),
            payload_kind: outcome.payload().kind(),
            payload: outcome.payload(),
        }
    }

    /// The envelope for a command that never reached the tool it names. Nothing ran, so the
    /// tool's exit is null and what `fraise` has to say is the whole payload.
    #[must_use]
    pub const fn refused(command: &'a str, tool: &'a str, exit: i32, payload: &'a Payload) -> Self {
        Self {
            ok: succeeded(exit),
            command,
            tool: Some(tool),
            exit,
            tool_exit: None,
            tolerated: None,
            payload_kind: payload.kind(),
            payload,
        }
    }

    /// The envelope as the one document it is.
    ///
    /// # Panics
    ///
    /// If it cannot be serialised, which is a bug in this module rather than a state a machine
    /// can be in: every payload it can hold was either parsed from JSON or is a string.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("an envelope serialises");
        json.push('\n');
        json
    }
}

/// Whether an exit in the umbrella's taxonomy is the one that means nothing went wrong.
///
/// The contract the classes come from numbers that class zero, and the process could only
/// return the number anyway, so this is the same judgement the shell makes.
const fn succeeded(exit: i32) -> bool {
    exit == 0
}

#[cfg(test)]
mod tests {
    use super::{Asked, Payload, PayloadKind};
    use crate::source;

    #[test]
    fn the_envelope_is_the_only_thing_that_writes_json() {
        // One envelope per command is a promise about output, so it is kept where output is
        // made: exactly one place in the crate turns a value into JSON text, and it is the one
        // that wraps it. A module that serialised its own report straight to standard output
        // would be a second shape for a caller to learn, and would fail here first.
        source::is_confined_to(
            &[
                source::spelled(&["serde_json", "to_string"]),
                source::spelled(&["serde_json", "to_writer"]),
            ],
            &[("envelope.rs", 1)],
            "writes JSON",
            "one document per command means one place that writes one, which is this module",
        );
    }

    #[test]
    fn bytes_that_parse_are_still_text_when_nobody_asked_for_json() {
        // The whole of D3 in one assertion: the same bytes, two answers, and the only
        // difference is what was asked.
        let bytes = br#"{"unions": 94}"#;
        assert_eq!(Payload::captured(Asked::Text, bytes).kind(), PayloadKind::Text);
        assert_eq!(Payload::captured(Asked::Json, bytes).kind(), PayloadKind::Json);
    }

    #[test]
    fn asking_for_json_does_not_make_an_answer_json() {
        let payload = Payload::captured(Asked::Json, b"compiling: 94 unions");
        assert_eq!(payload.kind(), PayloadKind::Text, "{payload:?}");
    }

    #[test]
    fn output_that_is_not_utf8_costs_a_character_rather_than_the_answer() {
        let payload = Payload::captured(Asked::Text, &[b'o', b'k', 0xff]);
        match payload {
            Payload::Text(text) => assert!(text.starts_with("ok"), "{text:?}"),
            other => panic!("captured text is text: {other:?}"),
        }
    }
}
