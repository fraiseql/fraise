# fraise

One command for the FraiseQL stack. `fraise` drives the four tools that turn a
spec into a running, migrated API — [fraiseql], [specql], [confiture] and
[fraisier] — and can always prove which versions of them it is talking to.

The umbrella owns the face; the tools keep their contracts. A person or an agent
learns one command, one config file, one JSON envelope, one `db/` layout and one
exit table instead of four of each.

## Status

Alpha, and honest about it: `fraise --version`, the exit table, `fraise doctor`
and the version-guarded `fraise tool` are what exist today. None of the named
verbs are implemented and the crate is not published. What is being built, in
order:

| | |
|---|---|
| `fraise.toml` | the one file a project author writes; `fraise config sync` renders confiture's YAMLs from it, and `--check` fails on drift |
| `--json` | one envelope for every command: the tool's raw exit beside the mapped one, and a field saying whether the payload is the tool's JSON or its text |
| the verbs | `init check build migrate deploy status up`, each documented as the exact tool invocation it performs, so falling back to the tool is always possible |

## The exit table

The four tools number their failures four ways, and only confiture's numbering is
a contract: nine semantic classes over exits 0–8, frozen, emitted as JSON by
`confiture --exit-codes-json`. `fraise` adopts that taxonomy whole rather than
inventing a tenth. `crates/fraise/src/exit_table.vendored.json` is that document
captured from the pinned release, **with the per-tool mapping inside it** —
fraiseql's 2 reads as 5, specql's 1 as 4 or 5 by error class, fraisier's 1 as 1 —
because a `match` statement drifts in silence and a document is diffed.

The confiture half is compared whole against the pinned confiture on every test
run, and a confiture that is missing, unanswering or a different release is a
**failure, never a skip**. The pin is `tools/confiture-requirements.txt` and CI
installs it before the gate, so the check is one that has actually run. Adopting a
confiture change is one commit that bumps the pin and regenerates the document
together; either half alone fails. The regeneration command is in the module
documentation of `crates/fraise/src/exit_table.rs`.

## The compatibility table

The four tools release on four schedules, so `crates/fraise/src/compatibility.toml`
states which release of each one this `fraise` may talk to, why that is the range,
and the command that installs an allowed release. `fraise doctor` executes each
tool's `--version`, reads what it printed and reports it against that statement:

```sh
fraise doctor          # a line per tool, with the reason and the fix for anything red
fraise doctor --json   # the same findings for a machine
```

Six verdicts, of which two satisfy the table. A version outside it, a tool that is
not installed, a version that cannot be read, and a build of a tool that has no
release at all are four different facts, and the report keeps them apart because a
reader acts differently on each. Anything unsatisfied exits with
`precondition_failed` — the class the table names and the exit contract defines,
which is the same refusal the version guard will use at a tool boundary.

Ranges are cargo semver, so a locally built `2.14.2-dev.<sha>` reads as outside the
table rather than as its release: a development build is a version nobody measured,
and being told so is more useful than a table quietly accepting it. specql is
marked as awaiting its first release; absent, that passes, and a build of it on
`PATH` is reported as vouched for by nothing.

CI installs the releases the table names — reading the commands out of
`doctor --json`, so there is one reader of the table — and then requires `doctor`
to accept them. That is what makes the table a contract rather than a preference.

## The version guard

Nothing reaches a tool without its version being read and judged first. The
judgement is the same one `doctor` reports, so a machine `doctor` calls green
cannot be one the guard refuses, and the reverse. `fraise tool` is the face that
reaches it directly — the fallback this README promises, available before the
verbs are:

```sh
fraise tool confiture migrate status        # version-guarded, exit mapped
fraise -C ../service tool fraiseql compile  # the tools run in the directory you name
```

A refusal exits with `precondition_failed`, the class the compatibility table
names and the exit contract defines, and says what was found, what was allowed and
how to install an allowed release. A tool's own exit arrives mapped through the
exit table — fraiseql's 2 is a validation failure, so `fraise` exits 5 — while an
exit the contract does not define passes through unchanged.

The child's working directory is always passed explicitly, never inherited by
accident. fraiseql's `compile` reads `fraiseql.toml` from the working directory,
and run from a sibling directory it emitted 0 unions of 94 at exit 0
(fraiseql#1387): a tool run in the wrong place can succeed at doing nothing.

### The escape hatch

A stack is sometimes mid-upgrade, and refusing to work is not always the kinder
answer:

```sh
fraise --allow-version-skew tool fraiseql compile
FRAISE_ALLOW_VERSION_SKEW=1 fraise tool fraiseql compile
```

What it will not do is tolerate a skew quietly: the version and the range it fell
outside are reported before the verb runs. It covers a version the table disagrees
with, and only that — a tool that is absent, or one whose version cannot be read,
is not a skew, and treating it as one would invent permission nobody gave.

This is also how specql is reached today. No release of it is published, so no
version of it is vouched for; a local build is refused until you say
`--allow-version-skew`, and that stays true until specql publishes a release the
table can name.

## Build

```sh
cargo build --release      # target/release/fraise
cargo xtask ci             # the gate CI runs: fmt, clippy -D warnings, tests
```

The gate measures the vendored exit table against a real confiture, so it needs
the pinned one on `PATH`:

```sh
uv venv --python 3.11 /tmp/confiture
uv pip install --python /tmp/confiture/bin/python -r tools/confiture-requirements.txt
PATH=/tmp/confiture/bin:$PATH cargo xtask ci
```

Requires stable Rust — the channel is pinned in `rust-toolchain.toml`, the MSRV
(1.95) is `rust-version` in `Cargo.toml`.

## Design rules

- A DSN is never printed and never passed on argv; secrets reach the tools as
  environment variables only.
- Every version that crosses a tool boundary is checked before the call, and an
  unexplained skew is refused rather than tolerated silently.
- Text that looks like JSON is not parsed as JSON.

## Licence

MIT.

[fraiseql]: https://github.com/fraiseql/fraiseql
[specql]: https://github.com/evoludigit/specql
[confiture]: https://pypi.org/project/fraiseql-confiture/
[fraisier]: https://github.com/fraiseql/fraisier-core
