# Phase 02: The binary

## Objective

One command that dispatches to the four tools and can always prove which versions of them
it is talking to.

## Success Criteria

- [x] `fraise --version` prints the crate version, under house lints (`unsafe_code` forbid,
      `missing_docs` deny, clippy all+pedantic+cargo deny, nursery warn)
- [x] The vendored exit table is confiture 1.19.0's `--exit-codes-json` compared **whole**,
      failing — never skipping — when confiture is absent or unpinned
- [x] The per-tool mapping (fraiseql 2 → 5, specql 1 → 4/5 by class, fraisier 1 → 1) is
      **inside that vendored document**, not a `match` statement (D7's condition)
- [x] `fraise doctor` finds each of the four binaries and reports its version against the
      compatibility table
- [x] **CI installs the four pinned binaries and runs `doctor` against the table**, so the
      table is in force and not merely configured (D2's condition)
- [x] **Every invocation that crosses a tool boundary checks the version first and refuses
      on a skew it has not been told to tolerate** (D2's condition)
- [x] The global `--json` envelope carries `ok`, `command`, `tool`, `exit`, `tool_exit`,
      `payload`, `payload_kind` — the tool's raw exit beside the mapped one, and the payload
      never heuristically parsed (D3's condition)
- [x] `cargo xtask ci` green; release-plz configured; one release tarball built in CI

## TDD Cycles

### Cycle 1: The crate and its version
- **RED**: an integration test runs the built binary with `--version` and asserts the
  `CARGO_PKG_VERSION` string. Fails: no crate.
- **GREEN**: workspace + `crates/fraise` + `xtask`, clap derive, house lints copied from
  pggit's skeleton (`Cargo.toml` workspace lints, `rust-toolchain.toml`, `release-plz.toml`).
- **REFACTOR**: one `Cli` type; every public item documented (`missing_docs` is deny).
- **CLEANUP**: `cargo xtask ci` green; `README.md`, `LICENSE` (MIT), `CONTRIBUTING.md`.

### Cycle 2: The exit table, vendored with its mapping
- **RED**: a freshness test that **fails** when `confiture` is missing, unpinned, or
  disagrees — comparing the whole document, not the integer→class map — plus a table-driven
  test over the mapping rows (fraiseql 2 → 5, specql 1 → 4/5 by class, fraisier 1 → 1)
  reading them **from the vendored document**. Fails: no document.
- **GREEN**: `tools/confiture-requirements.txt` pinning confiture 1.19.0 exact + transitive
  (the list fraiseql and fraisier-core already carry); capture `confiture --exit-codes-json`
  into `exit_codes.vendored.json`; add the `mappings` section; one loader.
- **REFACTOR**: no literal wire strings anywhere — `as_str` is held to the vendored classes,
  as fraisier-core's Cycle 4 ended up.
- **CLEANUP**: CI installs the pin before the gate.

*Pattern to copy verbatim: fraisier-core PR #65. Pin the tool, capture the JSON, compare the
whole document, fail rather than skip.*

### Cycle 3: The compatibility table and `doctor` — done 2026-09-26, CI green
*Landed as 4 commits (`87519cb` RED, `fc48bf7` GREEN, `3763994` REFACTOR, `00c4276` CLEANUP).
`compatibility.toml` + `compatibility.rs` (loader, `Verdict`, `Tool::judge`), `tool_version.rs`
(the one reader of a `--version`), `doctor.rs` (presentation only), and a `doctor` CI job that
installs what the table names by reading `doctor --json`. specql is `awaiting_release`: nothing
publishes one. Ranges are cargo semver, so a `-dev.<sha>` build reads as outside the table.*

- **RED**: `doctor` against stub binaries on a temporary `PATH` — one in-table version, one
  out-of-table — asserting what is reported and the exit. Fails: no `doctor`.
- **GREEN**: `compatibility.toml` as data (tool → allowed version range, with the reason);
  `doctor` execs each tool's `--version` and reports found / version / allowed.
- **REFACTOR**: one table loader shared with Cycle 4's guard.
- **CLEANUP**: a CI job installs the four pinned binaries and runs `fraise doctor --json`,
  failing the build when the table rejects them — **this is what makes the table a contract**.

### Cycle 4: The version guard on every tool boundary — done 2026-09-26, CI green
*Landed as 4 commits (`23bc635` RED, `decb1f2` GREEN, `49980ea` REFACTOR, `5fd63bf` CLEANUP).
`dispatch.rs`: `run` accepts only a `Cleared` that only `clear` hands out; the child's directory
is always explicit (fraiseql#1387); the hatch covers a disallowed version and not an absent tool
or an unreadable one — which is how specql is reached until it releases, now in the README. The
one-exec-path claim is a test over the shipped sources, proven to fail. `fraise tool <name>` is
the face; `tool_version::read` caches per process.*

- **RED**: a dispatch test where a stub tool's version is outside the table — `fraise`
  refuses **before exec'ing the verb** — and a second where the skew is explicitly tolerated
  (`--allow-version-skew` / `FRAISE_ALLOW_VERSION_SKEW`), which proceeds and records the
  tolerated skew in the envelope. Fails: dispatch has no guard.
- **GREEN**: one `Dispatcher` that execs `--version` (cached per process) before the verb.
- **REFACTOR**: the guard is the *only* path to an exec, enforced by a test rather than by
  convention.
- **CLEANUP**: document the refusal's exit class and its escape hatch in the README.

### Cycle 5: The envelope — done 2026-09-26, CI green
*Landed as 4 commits (`0fe5264` RED, `2d458ee` GREEN, `2838cdb` REFACTOR, and this one).
`envelope.rs`: `Asked` is what `fraise` asked the tool for and the only thing `payload_kind`
comes from (D3); `--payload <text|json>` on `tool` is how a caller says what its arguments
asked for, since `fraise` will not read a tool's flags on its behalf. Asked-for JSON that does
not parse is carried as text and the broken promise is said on stderr. Under `--json` the
child's stdout is captured and its stderr stays inherited, so progress still streams; without
`--json` nothing is captured and Cycle 4's behaviour is untouched. A refusal is an envelope
too. `doctor`'s report is the payload, which moved CI's jq to `.payload.tools[]`. The one-
serialiser claim is a test over the shipped source, proven to fail, sharing Cycle 4's scan.
Measured: clap's `requires` does not see a global given before the subcommand, so `--payload`
enforces its need for `--json` where it is read.*

- **RED**: two stub tools — one emitting JSON in a mode `fraise` asked for, one emitting text
  that *looks* like JSON — assert `payload_kind`, that the text one is **not** parsed, and
  that `exit` (mapped) and `tool_exit` (raw) both appear and can differ. Fails: no envelope.
- **GREEN**: the envelope type, emitted for every command under `--json`.
- **REFACTOR**: one serializer; the mapping comes from Cycle 2's document.
- **CLEANUP**: every field documented as what it means, not what it holds.

### Cycle 6: `cargo xtask ci`, release-plz, one tarball — done 2026-09-26, CI green
*Landed as 3 commits (`83759d9` RED, `f4f27a0` GREEN, `b1c0de2` REFACTOR and the cleanup this
note is part of). `tools/package.sh` is told which build, which target, which version and where
to leave the archive, and stages the binary, both tables and the licence under one directory:
`fraise-0.1.0-alpha.0-x86_64-unknown-linux-gnu.tar.gz`, named for the crate's version because
there is no tag on a version crates.io will not see. The judgement the cycle turned on is that
both tables are `include_str!`-compiled in, so a shipped copy is a second source — the test
searches the packaged binary for each document byte for byte, and for a one-byte drift of it to
prove the search bites. That check is in the test and not in the script because a shell one
cannot mean the same thing twice: `grep -F -f document binary` takes each line as its own
pattern, and `grep` here is ugrep, where `-z` searches archives instead of splitting on NUL.
`.github/workflows/release.yml` builds on a tag, on a release, on `workflow_dispatch` and on a
pull request that changes what a tarball is — the last because `workflow_dispatch` only fires
for a workflow already on the default branch, so without it this file's first run would have
been after it merged. zig floors the binary at glibc 2.28 and objdump holds it to 2.34. No
release-plz workflow: `gh secret list` on this repository is empty, so one added before the App
credentials would be a red build on every push to main.*

- **RED**: a test over `tools/package.sh`'s output asserting the tarball contains the binary,
  the vendored exit table, the compatibility table and the licence. Fails: no packaging.
- **GREEN**: the script, the release workflow, `release-plz.toml` (publishing off until the
  first alpha, as pggit's does).
- **REFACTOR**: `cargo xtask ci` is the single gate CI calls.
- **CLEANUP**: one tarball built and its contents asserted in CI.

## Dependencies

- Requires: the repository (D1) and confiture 1.19.0 on the pin. Nothing here waits on the
  five open PRs.
- Blocks: Phase 03 (`fraise.toml`), Phase 05 (the envelope across real tools).

## Not in this phase

No verbs (Phase 06), no `fraise.toml` (Phase 03), no `db/` layout work (Phase 04). `vhs` is
Phase 07's, but its install on forge or in the CI image is D8's standing action and should
not be discovered late.

## Status

[x] Complete 2026-09-26. Six cycles, 24 commits on `phase-02/the-binary`, both CI jobs and the
release workflow green; every box of fraiseql/fraise#1 ticked. What Cycle 2 measured and could
not fix is #3, which Phase 05 inherits.
