# fraise

One command for the FraiseQL stack. `fraise` drives the four tools that turn a
spec into a running, migrated API — [fraiseql], [specql], [confiture] and
[fraisier] — and can always prove which versions of them it is talking to.

The umbrella owns the face; the tools keep their contracts. A person or an agent
learns one command, one config file, one JSON envelope, one `db/` layout and one
exit table instead of four of each.

## Status

Alpha, and honest about it: `fraise --version` and the exit table are what exist
today. Nothing dispatches yet, no verbs are implemented, and the crate is not
published. What is being built, in order:

| | |
|---|---|
| `fraise doctor` | finds each of the four binaries and checks it against a pinned compatibility table, which CI runs the four pinned binaries against |
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
