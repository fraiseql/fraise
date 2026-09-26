# fraise

One command for the FraiseQL stack. `fraise` drives the four tools that turn a
spec into a running, migrated API — [fraiseql], [specql], [confiture] and
[fraisier] — and can always prove which versions of them it is talking to.

The umbrella owns the face; the tools keep their contracts. A person or an agent
learns one command, one config file, one JSON envelope, one `db/` layout and one
exit table instead of four of each.

## Status

Alpha, and honest about it: `fraise --version`, the exit table, `fraise doctor`,
the version-guarded `fraise tool`, the `--json` envelope and `fraise config show`
are what exist today. None of the named verbs are implemented and the crate is
not published. What is being built, in order:

| | |
|---|---|
| `fraise config sync` | rendering confiture's YAMLs from `fraise.toml`, with `--check` failing on drift. The file itself is read and shown today |
| the verbs | `init check build migrate deploy status up`, each documented as the exact tool invocation it performs, so falling back to the tool is always possible |

## Install

Not on crates.io: `publish = false` until the first alpha, because a published crate
that cannot do what its README describes is worse than no crate. Until then, from a
release tarball — the way that needs no Rust toolchain:

```sh
tar xzf fraise-0.1.0-alpha.0-x86_64-unknown-linux-gnu.tar.gz
install -m 0755 fraise-*/fraise ~/.local/bin/fraise
```

Beside the binary and the licence, the tarball holds the two documents that binary
was compiled against — `exit_table.vendored.json` and `compatibility.toml`, under the
names they have in the tree — so the range that refused a version on your machine is
a file you can read next to the binary that refused it.

Those copies are not taken on trust. Both documents are compiled into the binary, so
a copy is a second source, and a shipped compatibility table that read wider than the
binary's would call a machine ready that `fraise` refuses to dispatch on: the one way
a document shipped for the reader is worse than no document. So the packaged binary is
searched for each of them, byte for byte, and a tarball whose tables are not the ones
its binary carries fails the gate rather than shipping.

The Linux binary is linked against glibc 2.28 and held to a `GLIBC_2.34` ceiling, so
it loads on Debian 10, Ubuntu 18.04 and RHEL 8 and newer. A plain `cargo build` floors
a binary at the glibc of whatever built it, which is how fraisier's beta.4 tarball came
to be a release that would not load on Debian 12. One target is built today; the rest
arrive with the first published alpha. `tools/package.sh` builds one, and is told
everything it needs: which build, which target, which version, where to leave it.

## The one config file

`fraise.toml` is the file a project author writes, and the only one: `fraise` renders
what each tool expects from it. It has a `[project]` table, an `[environments.<name>]`
table per database, and one passthrough table per tool — the four the compatibility
table speaks for, and no others.

```toml
[project]
name = "printoptim"
default_environment = "local"

[environments.local]
database_url_env = "PRINTOPTIM_LOCAL_DATABASE_URL"

[confiture]
migrations_dir = "db/migrations"
notify_url = "${OPS_WEBHOOK_URL}"

[fraiseql]
schema = "public"
```

**An environment names the variable that carries its DSN, never the DSN.**
`database_url_env` must be an environment variable's name — `[A-Z_][A-Z0-9_]*` — and a
connection string there is refused. `fraise.toml` is committed; the DSN is not, and
`fraise` reads the variable at the moment it runs a tool. The refusal names the key and
does not quote the value, because a refusal that printed the DSN would put it in the log
the rule exists to keep it out of.

**A key nothing reads is refused, not ignored.** A tolerated one is a setting that
silently does nothing, and for a file that configures four tools that means a migration
run against something nobody asked for.

**`${VAR}` is confiture's rule, not one of ours**, because `fraise` renders confiture's
YAML: strict `[A-Z_][A-Z0-9_]*`, single pass, and a missing variable refused rather than
expanded to nothing. `${lower}`, `${VAR:-default}`, `${}` and an unclosed `${` are all
refused, each naming the setting it is in. The comparison against the pinned confiture,
row by row including the one row where the two deliberately differ, is the table in the
module documentation of `crates/fraise/src/interpolation.rs`. The passthrough tables are
where a value may come from the environment; the fields `fraise` reads itself — a
project's name, the environment to default to, the name of a variable — are read as
written and refuse a `${…}`. So no string in a loaded document keeps an unexpanded
reference.

Anything the file says that `fraise` cannot act on exits with `invalid_config`, the
class the exit contract gives this face's `invalid_configuration` refusal.

```sh
fraise config show          # what the file says, for a person
fraise config show --json   # the same, as the envelope's payload
```

What `show` prints is the document **as written**. A value that came from the
environment appears as the `${VAR}` reference the file holds, and a `from_env` map says
which variable fed which setting — which is what an agent needs in order to act, and is
not the secret. There is no masking rule to get wrong: a resolved value is never
rendered at all, and the type the report is made from cannot reach one.

```
printoptim — /home/you/printoptim/fraise.toml
default environment: local

environments
  local    PRINTOPTIM_LOCAL_DATABASE_URL
  staging  PRINTOPTIM_STAGING_DATABASE_URL

[confiture]
  migrations_dir  db/migrations
  notify_url      ${OPS_WEBHOOK_URL}

from the environment, and not shown here
  confiture.notify_url  OPS_WEBHOOK_URL
```

`fraise config sync`, which writes `confiture.yaml` and `db/environments/<env>.yaml`
from this file with a generated-file header, is not implemented yet.

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
`precondition_failed` — the class the exit contract gives this face's
`compatibility_unsatisfied` refusal, which is the same refusal the version guard uses
at a tool boundary.

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

## The envelope

Under `--json`, every command answers in one shape, so a machine learns the shape
once instead of learning four tools' output:

```sh
fraise --json doctor
fraise --json tool confiture migrate status
```

| field | what it means |
|---|---|
| `ok` | the command did what it was asked — the mapped `exit` being the contract's success |
| `command` | the verb of `fraise` that ran |
| `tool` | the tool it crossed into, or null when it crossed into none |
| `exit` | what `fraise` exited with, in the umbrella's one taxonomy |
| `tool_exit` | the tool's own number, unmapped. Null when no tool ran, and null when a signal ended one |
| `tolerated` | the version skew this command was told to let through, or null |
| `payload_kind` | `json`, `text` or `none` — what the payload is |
| `payload` | what `fraise` has to say: the tool's output, `doctor`'s report, or the refusal that stopped the command |

A refusal is a command that answered, so it is an envelope too, with the refusal
as its payload and a null `tool_exit` because nothing ran to produce one. A tool
ended by a signal is the other null: it ran and never returned a number, which
`tool` being named is what tells apart, and `exit` carries the shell's `128 + n`.

### The payload is never guessed at

`fraise` decides a payload is JSON from **what it asked the tool for**, never
from what came back. Text that looks like JSON is text:

```sh
fraise --json tool confiture --exit-codes-json                  # payload_kind: text
fraise --json tool --payload json confiture --exit-codes-json   # payload_kind: json
```

The two commands produce the same bytes and different envelopes, and the only
difference is that the second one said what it was asking for. That is the
caller's to say, because the caller wrote the tool's arguments and is the only
one who knows whether they asked it for a document; `fraise` will not read a
tool's flags on its behalf. Being asked is not enough either — output asked for
as JSON that does not parse is carried as text, and the broken promise is
reported on standard error rather than absorbed.

Under `--json` the tool's standard output is captured, because an envelope and a
tool cannot both own standard output. Its standard error is not: a tool that
streams progress keeps streaming it to the terminal as it is written. Without
`--json`, nothing is captured and the tool's output is its own.

## Build

```sh
cargo build --release      # target/release/fraise
cargo xtask ci             # the gate CI runs: shellcheck, fmt, clippy -D warnings, tests
```

The gate measures the vendored exit table against a real confiture, so it needs
the pinned one on `PATH`:

```sh
uv venv --python 3.11 /tmp/confiture
uv pip install --python /tmp/confiture/bin/python -r tools/confiture-requirements.txt
PATH=/tmp/confiture/bin:$PATH cargo xtask ci
```

Requires stable Rust — the channel is pinned in `rust-toolchain.toml`, the MSRV
(1.95) is `rust-version` in `Cargo.toml` — and `shellcheck`, which the gate runs over
`tools/` and which fails rather than skips when it is not installed. The scripts are kept
clean under 0.9.0, which is what the CI runner image carries, as well as under current
releases.

## Design rules

- A DSN is never printed and never passed on argv; secrets reach the tools as
  environment variables only, and `fraise.toml` names the variable that carries one
  rather than holding the string.
- A key nothing reads is refused rather than ignored, in every document this face
  owns: a tolerated one is a setting that silently does nothing.
- Every version that crosses a tool boundary is checked before the call, and an
  unexplained skew is refused rather than tolerated silently.
- Text that looks like JSON is not parsed as JSON.

## Licence

MIT.

[fraiseql]: https://github.com/fraiseql/fraiseql
[specql]: https://github.com/evoludigit/specql
[confiture]: https://pypi.org/project/fraiseql-confiture/
[fraisier]: https://github.com/fraiseql/fraisier-core
