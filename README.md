# fraise

One command for the FraiseQL stack. `fraise` drives the four tools that turn a
spec into a running, migrated API — [fraiseql], [specql], [confiture] and
[fraisier] — and can always prove which versions of them it is talking to.

The umbrella owns the face; the tools keep their contracts. A person or an agent
learns one command, one config file, one JSON envelope, one `db/` layout and one
exit table instead of four of each.

## Status

Alpha, and honest about it: `fraise --version` is what exists today. Nothing
dispatches yet, no verbs are implemented, and the crate is not published. What
is being built, in order:

| | |
|---|---|
| `fraise doctor` | finds each of the four binaries and checks it against a pinned compatibility table, which CI runs the four pinned binaries against |
| `fraise.toml` | the one file a project author writes; `fraise config sync` renders confiture's YAMLs from it, and `--check` fails on drift |
| `--json` | one envelope for every command: the tool's raw exit beside the mapped one, and a field saying whether the payload is the tool's JSON or its text |
| the exit table | confiture's canonical 0–8, vendored whole with the per-tool mapping inside the same document, and a test that fails rather than skips when confiture is absent |
| the verbs | `init check build migrate deploy status up`, each documented as the exact tool invocation it performs, so falling back to the tool is always possible |

## Build

```sh
cargo build --release      # target/release/fraise
cargo xtask ci             # the gate CI runs: fmt, clippy -D warnings, tests
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
