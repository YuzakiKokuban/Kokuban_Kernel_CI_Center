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
or device boot tests. rustfmt, clippy, and shellcheck are not installed in this
environment. Remote kernel repositories have not been pushed or synchronized by
this session; the prepared changes must be committed/deployed, then repository
setup/update will persist the hooks to their `bakasu` branches. Build-time
integration also handles source branches that have not yet been synchronized.

SUSFS options, defaults, source configuration, overlay downloads, patching,
release suffixes, and CLI/workflow plumbing were removed. Remaining references
are negative validation, regression coverage, and this migration description.
Upstream BakaSU itself is fetched unchanged; its optional driver-side code is
not maintained by this CI repository and is not enabled by this integration.
