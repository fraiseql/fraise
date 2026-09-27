# How `fraise` finds the database

Three of the four tools under the umbrella want a DSN, and each reads it its own way. So before
`fraise` runs any of them it answers one question — **which database is this command about** — and
answers it the same way every time.

That answer is not invented here. Confiture's **#152 precedence contract** already decides it, in
`python/confiture/cli/dsn.py::resolve_database_url`, and states its own principle in one line:

> explicit-and-singular wins; ambiguity fails loud

That file is byte-identical in the release this repository pins (1.19.0,
`tools/confiture-requirements.txt`) and in confiture's current 1.24.0 checkout, measured
2026-09-27. It is a contract rather than a release's behaviour, which is why `fraise` adopts it
whole instead of having a precedence of its own.

## The ladder

`fraise`'s inputs are not confiture's, so the contract is expressed through this face's statements.
The first statement in this list that an invocation made is the one that decides; the rest are not
consulted.

| rung | stated as | mirrors | what it means |
|---|---|---|---|
| `named_variable` | `--database-url-env NAME`, `FRAISE_DATABASE_URL_ENV` | step 1 | a source named for this command wins outright |
| `chosen_environment` | `--environment NAME` / `-e`, `FRAISE_ENVIRONMENT` | step 4 | an environment of `fraise.toml`, chosen for this command |
| `canonical_variable` | `CONFITURE_DATABASE_URL` | step 5 | confiture's canonical variable, set on purpose, beats a document that merely defaults |
| `default_environment` | `project.default_environment` | step 6 | the document's own default, which beats an ambient DSN |
| `ambient_variable` | `DATABASE_URL` | step 7 | whatever the environment happens to hold — refused for a command that changes the database |
| `nothing` | — | step 8 | no source, which is an answer: the tool's own configuration may still hold one |

Two of confiture's steps are refusals rather than rungs, and both are adopted:

- **Two explicit sources are never reconciled silently** (step 3, which confiture raises as
  `CONFIG_007`). An `--environment` *and* a `CONFITURE_DATABASE_URL` is a command nobody can
  resolve, so it is refused naming both. As in confiture, the check happens *below* the top rung:
  a `--database-url-env` wins before the question is asked, exactly as confiture's
  `--database-url` returns before its own conflict check.
- **A command that changes the database refuses an ambient-only DSN** (step 7 under
  `require_intentional_source`, which is confiture's own name for it).

Both of confiture's codes for these — `CONFIG_007` and `CONFIG_010` — are listed under one class in
its frozen exit contract, and that class is the one this face's `invalid_configuration` refusal
reads as. `tests/dsn.rs` asserts that against the vendored document rather than trusting the
number, so a contract that moved them reddens the suite instead of quietly giving these refusals a
different meaning from the tool whose rules they are.

## Four rules that follow

**A rung that answers, answers.** If the variable a rung named is not set, that is a refusal — not
a fall-through to the next rung. A command that quietly ran against another database is the failure
the whole ladder exists to prevent. Confiture's step 6 has the same shape: it defers to a config
that is merely *present*, without first checking that the config's own DSN is usable.

**An empty variable is not a source.** Confiture's resolver tests its two variables for truthiness
(`if confiture_url:`), so `CONFITURE_DATABASE_URL=""` is no source at all. That is *not* the rule
`${VAR}` expansion uses — there, a variable that is merely present expands, empty or not
(`if inner not in os.environ`). The difference is deliberate in confiture, so it is deliberate
here: `interpolation.rs` keeps the expansion rule and `dsn.rs` keeps the ladder's.

**An ambient DSN is never promoted to an intentional one.** When the source was named, `fraise`
hands the DSN to the child under every name the stack reads it by. When it was ambient, `fraise`
hands over nothing and leaves `DATABASE_URL` exactly as it found it. Copying it into
`CONFITURE_DATABASE_URL` would launder an accident into an intention, and confiture's own refusal
for a mutating command could never fire again.

**A `FRAISE_` override names a source, never a value.** The two overrides above are the environment
spellings of the two flags that name a source, and both hold a *name*: which environment, or which
variable. There is no `FRAISE_` spelling for a setting's value, and that is the rule rather than an
omission — the document is the one place a value is written, it is committed and it is diffed, and
an override that could set one would be a second document nobody can read. A value from the
environment already has a declared way in, `${VAR}` in the document, where a reader can see it. An
override still owes the report its `from_env` entry, because it displaced part of the document that
`config show` prints as written.

## What is handed to the tool

One DSN per invocation, reachable under every name the stack looks it up by — and always as
environment variables, never on argv, where every process on the machine could read it:

| name | why it is set | measured in |
|---|---|---|
| `CONFITURE_DATABASE_URL` | confiture's canonical, intentional source | `confiture/cli/dsn.py` (`_CONFITURE_DSN_ENV`), 1.19.0 |
| `DATABASE_URL` | what fraiseql's CLI resolves from after its own `--database` flag | `crates/fraiseql-cli/src/commands/run.rs`, 2.14.1 |
| the environment's own `database_url_env` | the name fraisier resolves the DSN through, since `[migration].database_url_env` in the config `fraise` renders for it carries that name | fraisier-core's Decision 5, `crates/fraisier-config/README.md` |

The first two names are a claim about another tool's interface, so they are measured rather than
remembered: a test runs the pinned confiture's `migrate up --help` and fails — never skips — if it
no longer documents both.

Two tools reading two different databases inside one command is the failure this translation
prevents. `specql` is handed nothing extra beyond that, because it wants no DSN.

## Seeing the answer without running anything

`fraise config show` reports the rung that would decide, the environment in force, the variable
that carries the DSN and whether it is set. It never reads the DSN, and it prints names and facts
only:

```
database
  rung         default_environment, which mirrors confiture's step 6
  environment  local
  variable     PRINTOPTIM_LOCAL_DATABASE_URL (not set)
```

`fraise --json config show` carries the same thing as `payload.database`, with the rung's reason
and the step it mirrors beside it. **Looking is not reading**: that a variable is unset is a fact
about the machine at this moment rather than about the document, so `show` reports it and exits 0.
The refusal belongs where the DSN is actually needed:

```console
$ fraise tool confiture migrate status
environment "local" names PRINTOPTIM_LOCAL_DATABASE_URL, and it is not set where fraise is
running. The DSN is not looked for anywhere else — a command that quietly ran against another
database is what this refuses — so export it, or name another source with --database-url-env (or
FRAISE_DATABASE_URL_ENV), --environment (or FRAISE_ENVIRONMENT), CONFITURE_DATABASE_URL, or a
project.default_environment in fraise.toml
```

An ambiguity *is* refused by `show`, because an invocation nobody could resolve is not a
configuration anyone can act on.

## Where this face deliberately differs from confiture

- **No `--database-url`.** Confiture's step 1 is a DSN on the command line; this face will not take
  one, because argv is readable by every process on the machine. Its top rung names the *variable*
  instead. Everywhere a DSN would be written, `fraise` takes the name of a variable.
- **No `--no-config`.** Confiture added it for runtime-resolved DSNs that must not appear in argv,
  which is already true of every DSN here. A `fraise.toml` that cannot be read is a refusal rather
  than something to suppress.
- **No parameter-source machinery.** Confiture needs `config_is_explicit` because its `--config`
  defaults to a *present file*, so the resolved path cannot tell a default from a choice. Here the
  default lives in the document — `project.default_environment` — so "the caller chose one" is
  simply `--environment` being given.

## Who says a command is mutating

`fraise tool` hands a tool arguments `fraise` did not write, and it will not read a tool's flags on
its behalf, so it cannot know whether they change the database. The caller says, with `--mutating`,
exactly as `--payload` is the caller's to say. The default is a reading, which is also confiture's
default (`require_intentional_source=False`), so the face is not stricter than the tool it fronts
for a `migrate status`.

When the named verbs arrive they state their own intent, because `fraise` writes their arguments —
and then this flag is what the fallback keeps for a tool invocation nobody wrapped yet.
