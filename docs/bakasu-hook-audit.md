# BakaSU hook audit — 2026-10-19

Scope: all 10 repositories in `configs/projects.json`, `bakasu` branch source
snapshots read through the GitHub connector. Source patches are maintained in
`configs/bakasu/`; `scripts/bakasu-hooks.py` applies them before builds, repository
setup, and upstream updates. Already complete trees are checked without applying
the patch again. A partially integrated or incompatible tree fails before patching.

| Project | Kernel | Source change | setuid / initrc |
| --- | --- | --- | --- |
| z5_sm8550 | 5.15.178 | exec, access, stat/fstat, reboot | automatic LSM |
| s25_sm8750 | 6.6.98 | exec, access, stat/fstat, reboot | automatic LSM |
| s23_sm8550 | 5.15.178 | exec, access, stat/fstat, reboot | automatic LSM |
| tabs9_sm8550 | 5.15.178 | exec, access, stat/fstat, reboot | automatic LSM |
| s24_sm8650 | 6.1.145 | exec, access, stat/fstat, reboot | automatic LSM |
| tabs10_mt6989 | 6.1.145 | exec, access, stat/fstat, reboot | automatic LSM |
| z6_sm8650 | 6.1.128 | exec, access, stat/fstat, reboot | automatic LSM |
| mi17_sm8850 | 6.12.69 | above + setresuid, read | explicit calls |
| razrfold_sm8845 | 6.12.38 | above + setresuid, read | explicit calls |
| floral_sm8150 | 4.14.355 | existing complete manual hooks | automatic LSM |

All use the automatic input handler. Exec integration includes pre/post hooks
and retains the filename across the consuming exec call. S25 and both 6.12 trees
use atomic filename references; the older trees use integer references.

Verification performed:

- Applied each of the nine patches with `patch --dry-run --fuzz=0`, then applied
  it to the actual source snapshot. Repeated integration passed for all 10 trees.
- All 10 patched snapshots passed BakaSU's own
  `kernel/tools/manual_hook_check.mk` from upstream commit
  `896e8981e0ed76ec14d1dcb39ab793438b304886`, with the appropriate automatic
  hook settings. The current upstream checker was also read through GitHub.
- Rust: 56 tests passed; `cargo build` and `cargo check` passed with Rust 1.92.0.
- Python: five regression tests passed, covering declarations/comments, version
  boundaries, every required hook configuration, repeat configuration, removal
  of retired settings, and rejection when they are enabled again.
- CLI wrapper and CI-core download/checksum/retry tests passed. Shell syntax,
  workflow YAML parsing, and `git diff --check` passed.
- Independent review found an update-path omission; both the Rust update command
  and reusable update workflow were repaired and reviewed again.

Limits: these are source/configuration and tool checks, not full kernel builds
or device boot tests. rustfmt, clippy, and shellcheck were unavailable locally during the initial
implementation. On 2026-10-20, the cloud Build CI Core workflow passed Rust
formatting, clippy, tests, binary compilation, checksums, and release publication.
All 10 remote `bakasu` branches are now synchronized and read back; their core
source blob hashes match the locally validated files. The 9 existing `main`
branches also have their retired config/payload fields and feature claims removed.
Pixel 4 has no `main` branch. Build-time integration remains active to cover
future source updates. See `bakasu-sync-2026-10-20.json` for exact commits.

SUSFS options, defaults, source configuration, overlay downloads, patching,
release suffixes, and CLI/workflow plumbing were removed. Remaining references
are negative validation, regression coverage, and this migration description.
Upstream BakaSU itself is fetched unchanged; its optional driver-side code is
not maintained by this CI repository and is not enabled by this integration.

## Remote synchronization — 2026-10-20

- CI implementation: `c78b643d059d7d83f9e84df1e0d05ab8b4bb7a65`.
- Rust format correction: `daa7ace41c62ad083c8d6c95c1137546631dccbf`.
- [Build CI Core passed](https://github.com/YuzakiKokuban/Kokuban_Kernel_CI_Center/actions/runs/39402920061), including release publication.
- Re-fetched all 10 source trees at pinned commit IDs, applied patches, checked
  repeat integration and ran the current upstream manual hook checker again.
- Read back every synchronized branch and checked configs and payloads for any
  remaining retired feature references. S25's stale menu comments were also
  removed, and the regression test now covers them.
- No full kernel compilation or device boot test was performed by this change.
