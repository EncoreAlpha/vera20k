# VMCE fork baseline

This branch tracks upstream `vera20k` and carries the optional VMCE
(`--features vmce`) campaign expansion. The baseline below is the state after
the upstream sync, not a native-parity certification.

| Item | Value |
| --- | --- |
| Upstream baseline | `1ea16933b` (Ares enabled from a game's data, #4319) |
| Merge commit | `70d569b86` (`Merge upstream/main (1ea16933b) into feature/vmce`) |
| Fork version | `0.3.2-vmce.6` (all three workspace manifests and `Cargo.lock`) |
| Capability | `vmce` (off by default; shipping builds do not enable it) |

## What the 0.3.2 merge changed

Upstream 0.3.2 (465 commits) brought the simulation thread (desktop frames tick
on a worker by default; the app reads a published presentation frame through
`SimHost::runtime()` and writes through `write_sim(SimWrite::…)`), Ares switched
on from game data (`RuleSet::ares`), Auto Repair, Team Starts / Fair Starts,
thirty lobby colours, the wider skirmish and lobby lists, the 1024x768 default
window, Android LAN, the Vietnamese catalog, and the removal of Render
Interpolation. The Mods menu is newer than this merge and arrives with a later
sync.

The VMCE side needed four kinds of adaptation: the result/defeat projections use
the new `SimHost` accessors; the ten i18n catalogs were merged key by key (all
fork and upstream keys present, no duplicates); the crate version became
`0.3.2-vmce.6`; and the architecture guards pin upstream's new Ares
superweapon-presentation reads and its rules-layer Ares switch setter.

## Resolution policy

Shared files follow upstream, then replay the VMCE work. The VMCE-only
surfaces that survive the merge:

- cooperative rounds: VERA's Next / CampaignEnd / Retry endings and ready
  gates. There is no retry grace timer or forced campaign-end pull;
- settlement agreement: the result record binds the round nonce to a digest of
  the resolved next-stage identity, and the local next-stage unlock is written
  only after every online participant confirms that agreement;
- LAN admission: the host advertises one content digest over the whole VMCE
  series (`campaign.ini` plus every manifest map's path, length and bytes) and
  a guest verifies it in a cancellable worker before reporting its map. The
  digest does not cover the rules or extension data (`RuleSet::ares`, mods), so
  peers must still run the same build and data (B102);
- campaign score `0x103`: upstream's shell slide path plus the VERA settlement
  board; the native Continue rect/input is unchanged. The board and statistics
  subpage are owned by `app::vmce_result` and read the finished world through
  `SimHost::runtime()`, which is the published presentation frame while the
  runtime is lent (B105);
- VMCE lobby: the flat native board, the setup 90-tick/high-credit presets
  from upstream and the VMCE-only role/country/colour columns. Colour claims
  follow the thirty-colour lobby list upstream added;
- `campaign.ini`: upstream Ares INI syntax (`+=`, `[Child]:[Base]`,
  later-wins) plus VERA's external empty-value overlay parser;
- Esc: upstream's settings dialog owns the key; VMCE lobby and settlement
  screens do not intercept Esc.

## Validation

The fork gate (`.github/workflows/vmce-tests.yml`) runs the upstream-registered
capability build and `python -m tools.capability_tests focus vmce` on Linux.
Locally the same build/test pairs are run for the four configurations
(default, `vmce`, `ares`, `vmce,ares`), together with the conflict-area tests
(Esc/settings input, setup presets, `0x103` renderer/layout, INI parser, house
defeat) and the i18n completeness suite.

The full VMCE validation has been re-run on the merged 0.3.2 baseline
(label `b5-sync-70d569b86-r1`, executable SHA-256 `404ce982…`, source
fingerprint `26dbb7ca…`):

- screenshot matrix: 13/13 at 800x600 (12 strict passes plus the seeded stock
  BC6 target under the repeat-run protocol), 2/2 at 1600x900, and 3/3 for the
  new 1024x768 default-window targets (`vmce-result-host-victory-next`,
  `vmce-result-host-victory-stats`, `vmce-defeat-host-disabled-2p`; byte- and
  stable-metadata-identical to the B2 baseline) — B103 closed;
- LAN acceptance: `2p-win`, `2p-retry`, `4p-win`, `2p-final` all passed
  (93/93 checks, 300 matching checksums per scenario, next-stage/rows/seed
  receipts) plus the stock Cooperative continuation run;
- desktop frames tick on the simulation worker by default on this baseline;
  the live victory board and the live defeat statistics page rendered
  non-empty, with equal host/guest result fields and state hashes — B105
  closed;
- focused Rust tests: 332 passed (default) and 459 passed (`vmce`), including
  the architecture guards.

The pre-sync B4 run (`6322d411b`) remains the comparison baseline: its matrix
was 15/15 under the repeat-run protocol, its LAN acceptance 93/93, and its
stock continuation passed; the merged run above repeated the same set on the
new build. Evidence: `.scratch/b5/sync-acceptance/` (summary, comparison
JSONs, receipts).

The shipped i18n baseline keeps every catalog the fork maintains key-complete:
English carries the real VMCE labels, zh-CN keeps the fork's translations, and
the other eight language files use the English text as explicit placeholders
until translated. Upstream's Vietnamese catalog, added in 0.3.2, has no VMCE
keys yet (B101).

## Real-process captures

The runner selects `4p-win`, `2p-win`, `2p-retry` and `2p-final`; `--players`
controls 2, 3 or 4 rows for the `4p-*` scenarios. It compares the 300-frame
checksum stream and scenario-specific receipt fields; each run uses a single
supplied series, which must suit the selected scenario. Two-, three- and
four-player wins and next missions, a two-player retry and the `2p-final`
scenario passed on the merged baseline (the same set passed on the pre-sync B4
base before it), and on the pre-sync base each peer's local
`VmceProgress.json` was checked to contain only the designated next stage
(`2p-final` writes none, because the last stage resolves no successor). The
final-stage run reached credits and returned after Escape; the full credits
duration was not observed. Run one scenario per invocation on machines with
limited shared GPU memory: an exited capture process can still hold its device
allocation, and a following scenario may fail inside wgpu with "Out of Memory"
although each scenario passes separately. Bundles stay under `.scratch/`; the
merged-baseline bundles are under `.scratch/b5/sync-acceptance/`.

## Re-capturing and re-accepting

The capture and comparison runners are machine-local scripts under the gitignored
`.scratch/` tree; this section documents the procedure, not committed code.

- **Shell-capture matrix** (`.scratch/b3/capture_matrix.py`): launches a labelled
  `vera20k.exe` against an isolated runtime directory, writes `frame.bgra`,
  `capture.json`, logs and a `<target>.run.json` receipt per target, and never
  edits the repository. It takes `--exe`, `--label`, `--label-manifest`,
  `--runtime`, `--out-root`, `--targets`, `--width`, `--height`, `--cursor-x`,
  `--cursor-y`, `--capture-timeout` and `--suffix`. The runtime directory must
  contain `config.toml`; the VMCE result/defeat fixtures additionally need an
  authored-role map under `settings/Maps/` (B91 workaround).
- **Comparison** (`.scratch/b3/compare_matrix.py`): compares a baseline run set
  with a candidate set under the accepted repeat-run protocol (byte-identical
  frames plus stable metadata; known animation-phase and seeded-content
  variants compared as sets; same-environment cross-executable controls for
  the fitted-window pair).
- **LAN acceptance** (in the repo, `tools/vmce_lan_acceptance.py`): drives the
  VMCE checkpoints through real processes; `run` and `compare` subcommands,
  `--scenario`, `--players` and `--entry` as described in the
  [overview acceptance section](vmce.md#accepting-a-build). Bundles stay under
  `.scratch/`.
- **Stock Cooperative continuation** (`.scratch/b3/run_stock_coop.py` plus the
  official comparator): the pre-sync regression guard for the ordinary
  Cooperative retry path.

The merged 0.3.2 baseline has been re-captured with these runners (matrix at
800x600, 1600x900 and 1024x768; LAN `2p-win`/`2p-retry`/`4p-win`/`2p-final`;
stock continuation). Re-run the same set after the next upstream sync or any
change that can affect rendering or simulation timing.

## Audit checkpoint

The [Chinese consolidated review](zh-CN/vmce-review.md) is the authoritative open-defect list;
it currently holds 55 entries (45 `A` findings plus 10 B-only residuals), and the
[backlog](vmce-backlog.md) adds the five post-0.3.2 baseline items B101–B105
(B103 and B105 closed by the merged-baseline run). The current audit's
local capture set is `2p-win`, `2p-retry`, `4p-win` with 2, 3 and 4 players, `2p-final` and the
manifest-extra 2p-final probe; a preview-capable host lobby was also captured with
`XMP29U2.MAP` as a series stage. These establish local multi-process behavior only.
Cross-machine play, guest-side intermission preview (A44), the full-room LAN listing (A45) and
long-name statistics (A38) remain uncovered; the synthetic result/defeat fixtures additionally
need the backlog's B91 workaround.
