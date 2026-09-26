# Phase 02: The binary

## Objective

One command that dispatches to the four tools and can always prove which versions of them
it is talking to.

## Success Criteria

- [ ] `fraise --version` prints the crate version, under house lints (`unsafe_code` forbid,
      `missing_docs` deny, clippy all+pedantic+cargo deny, nursery warn)
- [ ] The vendored exit table is confiture 1.19.0's `--exit-codes-json` compared **whole**,
      failing — never skipping — when confiture is absent or unpinned
- [ ] The per-tool mapping (fraiseql 2 → 5, specql 1 → 4/5 by class, fraisier 1 → 1) is
      **inside that vendored document**, not a `match` statement (D7's condition)
- [ ] `fraise doctor` finds each of the four binaries and reports its version against the
      compatibility table
- [ ] **CI installs the four pinned binaries and runs `doctor` against the table**, so the
      table is in force and not merely configured (D2's condition)
- [ ] **Every invocation that crosses a tool boundary checks the version first and refuses
      on a skew it has not been told to tolerate** (D2's condition)
- [ ] The global `--json` envelope carries `ok`, `command`, `tool`, `exit`, `tool_exit`,
      `payload`, `payload_kind` — the tool's raw exit beside the mapped one, and the payload
      never heuristically parsed (D3's condition)
- [ ] `cargo xtask ci` green; release-plz configured; one release tarball built in CI

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

### Cycle 3: The compatibility table and `doctor`
- **RED**: `doctor` against stub binaries on a temporary `PATH` — one in-table version, one
  out-of-table — asserting what is reported and the exit. Fails: no `doctor`.
- **GREEN**: `compatibility.toml` as data (tool → allowed version range, with the reason);
  `doctor` execs each tool's `--version` and reports found / version / allowed.
- **REFACTOR**: one table loader shared with Cycle 4's guard.
- **CLEANUP**: a CI job installs the four pinned binaries and runs `fraise doctor --json`,
  failing the build when the table rejects them — **this is what makes the table a contract**.

### Cycle 4: The version guard on every tool boundary
- **RED**: a dispatch test where a stub tool's version is outside the table — `fraise`
  refuses **before exec'ing the verb** — and a second where the skew is explicitly tolerated
  (`--allow-version-skew` / `FRAISE_ALLOW_VERSION_SKEW`), which proceeds and records the
  tolerated skew in the envelope. Fails: dispatch has no guard.
- **GREEN**: one `Dispatcher` that execs `--version` (cached per process) before the verb.
- **REFACTOR**: the guard is the *only* path to an exec, enforced by a test rather than by
  convention.
- **CLEANUP**: document the refusal's exit class and its escape hatch in the README.

### Cycle 5: The envelope
- **RED**: two stub tools — one emitting JSON in a mode `fraise` asked for, one emitting text
  that *looks* like JSON — assert `payload_kind`, that the text one is **not** parsed, and
  that `exit` (mapped) and `tool_exit` (raw) both appear and can differ. Fails: no envelope.
- **GREEN**: the envelope type, emitted for every command under `--json`.
- **REFACTOR**: one serializer; the mapping comes from Cycle 2's document.
- **CLEANUP**: every field documented as what it means, not what it holds.

### Cycle 6: `cargo xtask ci`, release-plz, one tarball
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

[~] Planned 2026-09-25. Cycle 1 begins once the repository exists.
