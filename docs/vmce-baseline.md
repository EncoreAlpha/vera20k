# VMCE fork baseline

This branch tracks upstream `vera20k` and carries the optional VMCE
(`--features vmce`) campaign expansion. The baseline below is the state after
the upstream sync, not a native-parity certification.

| Item | Value |
| --- | --- |
| Upstream baseline | `867ae2467` (upstream main, 2026-11-13; room Teams/join queue, mods page, Ares in every build behind its switch) |
| Merge commit | `89ccbea49` (`Merge upstream/main (867ae2467) into feature/vmce`) |
| Fork version | `0.3.2` (all three workspace manifests and `Cargo.lock`); the fork no longer suffixes the upstream version, so the `vmce` capability identifies a VMCE build |
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
fork and upstream keys present, no duplicates); the crate version carries the upstream
`0.3.2` (the fork no longer suffixes it); and the architecture guards pin upstream's new Ares
superweapon-presentation reads and its rules-layer Ares switch setter.

## Latest upstream sync (2026-11-13)

Merged upstream main `867ae2467` (59 commits after 0.3.2) as `89ccbea49`; the
per-file conflict resolutions are in that merge message. Validation on the
merged tree: `check -p vera20k --lib --bins` with and without `--features
vmce`, architecture guards 26/26 in both configurations, focused VMCE tests
224, stock Cooperative/setup/LAN default tests 146, and a labelled VMCE build
(`vmce-upstream-867ae2467-r1`, executable SHA-256
`160163127F0FA58186456F88C06188113DA721BDB5F77295EA84AF5A51179749`) passing the
real two-process `2p-win`, `2p-retry` and `2p-final` acceptance (300 matching
checksums each; start, result, retry, intermission continue and campaign end).
The 20,000-unit performance comparison and the VMCE+Ares combination were not
run. Decided 2026-11-13: keep VMCE as a compile-time `vmce` capability per the
roadmap; this is an explicit exception to the upstream AGENTS policy that game
behavior does not sit behind a build feature.

Post-audit fixes (`567f5f59c`): the shared building-admission predicate is back
on `owner_is_human` (VMCE input keeps its local grant predicate), the fork
reports upstream version `0.3.2` so Mods `Needs=` parses, the Vietnamese
catalog carries `VERA:LanSetupApplyFailed`, and the three VMCE guard comments
are restored. The rebuilt `vmce-audit-fixes-0.3.2-r1` executable (SHA-256
`73688C0489A54478F9BB464143C1D373EB16E3FA0A6C3A359E431204F32CCF01`) re-passed
the real two-process `2p-win`, `2p-retry` and `2p-final` acceptance with 300
matching checksums each.

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
  series digest covers `campaign.ini` and the manifest maps; the separate LAN rules
  agreement covers the selected rules closure and the Ares/extension build identity, and the
  VMCE setup handshake refuses a peer whose build or rules differ before the mission starts
  (B102 closed);
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
until translated. The 2026-11-13 upstream sync added the 32 VMCE keys to the
Vietnamese catalog as English placeholders; translating them remains open (B101).

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
## Phase 2 �?VMCE-owned settlement flow (feature/vmce)

VMCE now owns the result round end-to-end: its own session phase
(`NetworkPhase::VmceResult` in `app/match_runtime/session/vmce_result.rs`), result
agreement (propose/confirm with nonce and stage digest), the victory/campaign-end
projections, a VMCE-owned defeat/retry page (`app/vmce_result/defeat*.rs` and
`app/frontend/skirmish_shell_render/vmce_defeat.rs`), the LAN result service
(`app/lan_service/vmce_round.rs`) and the app steps for Next, Retry, CampaignEnd
and return to shell (`app/vmce_round.rs`). The stock Cooperative round state,
events, UI model and page are no longer opened for a VMCE match; the stock BC6
result owner keeps handling ordinary Cooperative results. Shared VERA pieces
still used: session roster/lobby admission, reliable LAN queue, simulation
score rows/difficulty and base shell controls.

Protocol: result records carry the nonce, per-slot terminal entries, a
confirmation flag and a stage digest over the resolved next stage; Ready/Leave
and retry ACKs are VMCE signals, Go is the nonce-scoped result control. Retry
uses an explicit guest ACK gate before the host freezes into the reload; no BC6
Begin/Ack/Options handshake is required. Old VMCE protocol/checksums are not a
compatibility requirement for this stage.

Architecture: `architecture_guards::vmce_result_owners_do_not_call_the_cooperative_result_flow`
forbids `cooperative_round::`, `ui::coop_score_shell`, `lan_service::cooperative::`
and Cooperative result-page types in the VMCE result owners; the existing
feature-gate budget was re-pinned for the new shared call sites.

Evidence (local, labelled fast build `vmce-phase2-final-r3`, SHA-256
`C26BD3C9AAEBCA9B757BFC4619124E8A634B2E02F639FAACFFA261A7E256AA78`):
`2p-win`, `2p-retry` and `2p-final` LAN acceptance all PASS under
`.scratch/final-accept-r3/`, with 300 checksums per peer for the retry comparison
and the expected next-stage/campaign-end receipts. `architecture_guards` passes
with and without `vmce`; `check --workspace --lib --bins --features vmce` and
`check -p vera20k --lib --bins` pass. Full `vera20k --lib` suites: default
3988 pass / 8 known environment-retail failures (same set as the pre-phase
audit), VMCE feature 4093 pass / the same 8 failures; both known sets reproduce
on the unmodified upstream checkout.

Residual: the generated defeat page reproduces the BC6 backdrop, right panel,
four-row statistics table and Retry/Leave slots with VMCE-owned layout code, but
the native BC6 statics animation and the exact per-pixel static placement are
not ported (no entry slide/animation; whole-pixel differences in table text
placement). The retry/reconnect path relies on the reliable setup queue's
retransmission for a lost Go; guest ACK loss is retried by the same queue, and
the LAN transport has no separate VMCE ACK-resend timer. Cross-machine play and
internet play are outside this stage.

Critic findings (fixed in the final revision): assignment now requires the full propose/confirm agreement, a stale BC6 Ack no longer commits a result, and the defeat page carries Your Time/Par Time labels, the per-row Lost verdict, name ellipsis and a pointer press frame.

Remaining, not fixed in this stage: no local retry-ACK deadline or dedicated resend timer (reliable setup queue only); a guest whose host never assigns has no result-page timeout without a received intermission epoch; no hover status/STT row on the VMCE defeat page; the native BC6 statics animation and exact static placement are not ported; superseded BC6 helper paths and the old encode_scoped_request stub remain in the LAN adapter; the architecture guard scans five VMCE-owned paths and has no negative fixture (it proves absence of named Cooperative calls, not full isolation); LAN diagnostics call page actions directly and hard-code verdict/can-continue, so hover/press/hit-test and an old-BC6 pixel comparison remain uncovered. Cross-machine and internet play stay out of scope.

## Phase 2 review fixes (feature/vmce)

**Superseded by the acceptance fixes below.** The bounded two-phase retry
handoff described in this section waited for the reliable setup FIFO's transport
drain before the host froze; the acceptance-fix section below replaces that
proof with an application-level commit acknowledgement and closes the
result-page/loading-stage fault path. The text below stays as the r6 record:
the host queues the nonce-scoped retry Go, waits for every guest RetryAck, then
queues a RetryCommit on the same reliable setup FIFO and only reloads after that
commit has drained; a guest that sees Go queues its ACK and stays on the result
page until RetryCommit arrives. ACK loss, commit loss, a full reliable queue, a
transport failure, or an absent peer all end in a local visible
`VERA:VmceRoundSendFailed` return to the LAN lobby on both peers (600-bucket
deadlines); neither side reloads while the other waits. The new control and its
kind are VMCE-owned (`VmceRoundControl::RetryCommit`); the current protocol is
in the acceptance-fix section below.

The native BC6 Escape behavior is restored on the VMCE defeat page: Escape does
not leave the room; only the explicit Leave Room control does. The LAN capture
calls the window handler's own dispatch function (the same function
`handler.rs` uses for the CoopScore screen), asserts the page is not in a
blocked transition, and records `escape_preserved_page` in both receipts.

Cooperative dependency cleanup: the shared continuation timing is owned as
`net::lockstep::continuation_timing` (the native Cooperative timing values are
transport timing, not a Cooperative result decision); VMCE result messages have
their own `SIGNAL_LEN` and no longer import Cooperative options or payload
types. Remaining shared helpers whose names still begin with `cooperative_`
are simulation score rows and campaign difficulty (and the generic
`continuation_timing`); the roster-admission allowlist entries were removed
once no VMCE-owned caller used them.

The architecture guard now scans VMCE production files, VMCE session tests and
marked VMCE diagnostic regions, forbids the old timing name, bare and aliased
`cooperative_round` module paths and Cooperative message/state types, and has a
combined negative fixture test that probes every forbidden token plus an alias. It is intentionally
source-level: it proves the named dependencies are absent, not that every
future indirect call is impossible.

Evidence (local, labelled fast build `vmce-phase3-final-r6` with
`vmce,dev-overlays`, SHA-256
`3268D6FF77E523C8AFF13EFB7F62ACF40C840ED97DBBD9DA477E2284A45A8D17`):
`2p-win`, `2p-retry` and `2p-final` LAN acceptance all PASS under
`.scratch/phase3-accept-r6/`; each receipt records 300 native checksums, the
retry receipt records the same stage on both peers, and both receipts record
`escape_preserved_page: true`. The retry run additionally proves the new
handoff, because the old direct-Guest freeze would not wait for RetryCommit.

Focused tests added: real reliable-queue ACK loss and full-queue rejection
(`lan_service` tests), lost VMCE commit/ACK datagrams (`global_tests`), host and
guest bounded handoff decisions, and the session Go/RetryCommit state machine.
Retail checks: default and VMCE `cargo_run check --lib --bins` pass; workspace
check with `vmce` passes; `architecture_guards` passes both ways (28 tests);
full `vera20k --lib`: default 3989 pass / 8 known environment-retail failures,
VMCE 4102 pass / the same 8 failures. Still uncovered: cross-machine play,
internet play, a guest that hangs while the host transport stays healthy (no
pre-Go deadline by design), and an acceptance run with an externally injected
lost ACK; the
in-process reliable-queue and global-channel tests drop/withhold packets but
the LAN scenarios do not manipulate the OS network path. The shared generic
helpers whose names still begin with `cooperative_` are simulation score rows,
campaign difficulty and roster admission, not the Cooperative result flow. The retry capture still drives the page action directly for the
Retry click (only Escape goes through the input handler), and there is no
old-BC6-versus-new-page pixel baseline comparison.

## Phase 2 acceptance fixes (feature/vmce, 2026-11-13)

The r6 two-phase retry handoff still treated the reliable setup FIFO's transport
drain as proof that a guest had applied RetryCommit. A delivered commit whose
transport ACK was lost timed the host out, and a peer that never received the
commit could leave the other peer loading alone. This revision replaces that
proof and closes the result-page/loading-stage fault path:

- **Retry handoff is three-phase and VMCE-owned:** host Go -> guest RetryAck
  (existing), host RetryCommit -> every guest applies it and answers a new
  application-level `RetryCommitAck` without reloading, host retransmits the
  commit every 120 buckets (a lost transport ACK or a lost application ACK is
  repaired by a new commit record), then host queues `RetryStart` and freezes;
  guests reload only on RetryStart. A queue drain is never application proof.
  The overall commit bound (600 buckets) is set once and no resend can extend
  it; host ACK wait, guest waits and duplicate Go/commit/Start records are
  bounded and idempotent.
- **Application liveness:** while a VMCE result round is open each peer sends a
  VMCE heartbeat every 60 buckets. A healthy but idle human keeps the other side
  waiting indefinitely; 600 buckets of silence ends the round in bounded time
  with `VERA:VmcePeerUnresponsive`. During the bounded retry handoff the
  handoff's own deadline owns the verdict, so an unreliable-heartbeat gap
  cannot pre-empt it.
- **Visible bounded failures:** full queue, transport failure, partial commit
  with two or more guests, unresponsive peer and lost commit/ACK all end through
  the existing LAN departure owner: peers are notified first, both show a
  localized reason (`VERA:VmceRoundSendFailed` / `VERA:VmcePeerUnresponsive`),
  and the room returns to the LAN lobby. Network faults can temporarily leave
  peers in different phases; the loading failure path provides a bounded exit.
- **Loading stage:** a VMCE LAN start/retry/next-stage load whose peer wait times
  out no longer installs the mission with the straggler dropped (native stock
  behavior). It aborts through the existing VMCE loading-failure owner with
  `VERA:VmceLoadPeerTimeout`; stock Cooperative and native loading keep their
  original behavior.
- **Result diagnostics split:** the whole VMCE live result diagnostic
  (`.../live/vmce_result.rs`, including the `--vera-outcome` injection/arm tail)
  and the score-capture VMCE fixtures (`.../score/vmce_fixture.rs`) are
  VMCE-owned files with no marked regions and no Cooperative result calls; the
  stock Cooperative diagnostics stay in `live/cooperative.rs`/`score.rs`.
  `--vera-outcome win/lose` now waits for VMCE's own pages (the `0x103` victory
  campaign-score page with the VMCE board/statistics, and the VMCE defeat page)
  instead of the old `coop_score_page`, which VMCE never opens, and quits
  through VMCE Leave actions.
- **Architecture checks:** whole-file ownership of VMCE production files
  (including the VMCE wire module and this phase's vmce-named production
  functions in shared files), the whole VMCE session tests, VMCE test functions
  in shared test files, and no marked regions. The guard enforces the documented
  shared-capability allowlist and walks resolvable source-level calls, with
  negative fixtures for direct, aliased and selected wrapper references. It
  cannot resolve every receiver type or deeper ambiguous method chain, so a
  read-only review remains necessary. Generic shared capabilities (`release_transport`, the
  renamed generic `DepartureReason::RoundResult`, reliable queue/transport,
  session/roster accessors, simulation score rows/difficulty and continuation
  timing) are listed with responsibility and callers.
- **Unchanged:** Esc on the VMCE defeat page, result-page layout, the existing
  win/retry/campaign-end flow, and simulation/checksums.

Evidence (local, labelled fast build `vmce-phase3-accept-r8` with
`vmce,dev-overlays`, executable SHA-256
`FB2FB8C8FBD2A6C11D60F445D53156611EE6227160E7D475D2FF2C22BADB7C1C`, source
fingerprint `afc5d3eb0c8b4bb4841232dae23b76aa417b27606424a60ac6f55c41aba9b5de`):
`2p-win` (LDA-Continue-real stage01), `2p-retry` (LDA-Retry stage01) and
`2p-final` (LDA-2p-final stage02) all PASS under `.scratch/phase3-accept-r8/`
with 300 matching checksums per peer and the expected next-stage, retry-reload
and campaign-end receipts; these runs also exercise the normal
`--vera-outcome real`/`retry`/`end` paths through the moved VMCE result
diagnostic and its VMCE page predicates. Stock Cooperative `retry` and
`advance` real-process runs PASS. Focused tests, both feature configurations:
architecture guards 41/41, `app::lan_service` 91, `app::lan_service::vmce_round`
14, `vmce_result` 4, `app::loading::pump` 51 (+2 pre-existing ignored),
`app::diagnostics` 184 with `vmce,dev-overlays`, core `net::global` 12/13,
`net::lan_messages` 18, and i18n shipped-language completeness;
`check -p vera20k --lib --bins` passes with and without `vmce`.

The follow-up repair starts result-round liveness before the guest receives its
first result nonce and preserves a bounded opportunity to deliver a failure
notice when the setup FIFO is backpressured. The injected diagnostic continues
observing committed frames after its prefix; on the authored test maps it records
the input lock rather than claiming to have issued Move/Stop. Local two-process
`win` and `lose` captures both reached the expected VMCE page, with 300 matching
checksums and an actual injection frame of 500 on each peer. The same fast build
passed `2p-win`, `2p-retry` and `2p-final` with 300 matching checksums. The two
new network regression tests and diagnostic phase test were seen to fail with
the corresponding fixes temporarily reverted and pass with the fixes restored.

Residual: cross-machine and internet play and OS-level packet loss were not run;
the in-process tests drop specific datagrams/ACKs through the reliable channel.
A process paused by the OS for more than about 9.6 s is declared unresponsive.
The source-level guard cannot resolve every receiver type: deeper ambiguous
method wrappers, dynamic dispatch, function pointers and generated code need
read-only review. VMCE setup/intermission diagnostics and remaining Cooperative
dependencies in the stage-3 lobby/setup path are follow-ups; non-retry
intermission/campaign-end Go delivery keeps its existing reliable-drain wait.
