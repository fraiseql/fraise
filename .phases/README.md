# fraise — the umbrella binary

Step 2 of `~/code/fraise-stack/STRATEGY-2026-09-21.md` §5, program plan in
`~/code/fraiseql-face/.phases/2026-09-22-one-face/README.md`. D1 (2026-09-25) put this
crate in its own repository, `github.com/fraiseql/fraise`, under a freeze exception
written into §5 itself.

`.phases/` is on disk only here, as in fraiseql, specql and pggit: it is gitignored, and
the finalize phase leaves no trace of it in the tree.

| Phase | Title | Status |
|---|---|---|
| 02 | The binary | [~] Cycles 1–5 landed 2026-09-26 (CI green); Cycle 6 open |
| 03 | One config (`fraise.toml`, `config sync --check`) | [ ] |
| 04 | One layout (`db/` tenant-zero tree in all three generators) | [ ] |
| 05 | One JSON, one exit (envelope across all four tools) | [ ] |
| 06 | The verbs (`init check build migrate deploy status up`, `fraise mcp`) | [ ] |
| 07 | The recorded agent demo (`vhs` tape, CI render, demo.fraiseql.dev) | [ ] |
| XX | Finalize | [ ] |

Phases 03–07 live in the program README; this file tracks them only as they land here.
