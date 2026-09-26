# Contributing

## The cycle

Tests come first. A change arrives as a failing test that fails for the reason
it claims, then the smallest code that passes it, then a refactor, then a clean
gate — in that order, and the commit messages say which is which.

## The gate

```sh
cargo xtask ci
```

Format check, clippy with `-D warnings` over `--all-targets --all-features`, then
the test suite. CI runs this one command, so a green gate locally is a green CI.

## House rules

- `unsafe_code` is forbidden; `missing_docs` is denied; clippy runs at
  `all + pedantic + cargo` denied and `nursery` warned. An `#[allow(...)]` needs a
  `// Reason:` comment above it.
- Stable Rust only. Nightly-only formatting options are deliberately absent from
  `.rustfmt.toml` so that the formatter is the same everywhere.
- Conventional commit subjects (`feat`, `fix`, `refactor`, `test`, `docs`,
  `chore`, `ci`), a `## Changes` list and a `## Verification` list.
- Never push to `main`; open a pull request.
- A vendored copy of another tool's document (the exit table, the compatibility
  table) gets a freshness test that **fails** when the tool is missing or
  disagrees. It must never skip.
