# VMCE fork baseline

This branch tracks upstream `vera20k` and currently carries the compile-gated VMCE
(`--features vmce`) campaign expansion. The candidate first entry-to-start migration slice is
compiled in every build; the remaining VMCE reader, loading and gameplay backend still uses the
feature. The baseline below is not a native-parity certification or a completed target design.

| Item | Value |
| --- | --- |
| Upstream baseline | `0f7dc1bbc` (upstream main, 2026-11-14; version 0.3.3: Ares behind runtime switches in every build, draw/radar and display tables, LAN name-box bytes and fixed chat colour, random-map extras) |
| Merge commit | `78b0aef4a` (`Merge upstream/main (0f7dc1bbc) into feature/vmce`) |
| Fork version | `0.3.3` (all three workspace manifests and `Cargo.lock`); the fork no longer suffixes the upstream version, so the `vmce` capability identifies a VMCE build |
| Capability | Remaining VMCE backend: `vmce` feature, off by default. Candidate entry-to-start slice: mode ID/parser in every build; visibility and GO permission derive from validated `ScenarioCatalog` entries. Full migration remains open |

## Latest upstream sync (2026-11-15)

Merged upstream main `0f7dc1bbc` (191 commits after `867ae2467`, version
0.3.3) as `78b0aef4a`; the per-file conflict resolutions are in that merge
message. Upstream brought Ares behind runtime switches in every build, the
draw/radar and display-table work, LAN name-box byte handling and fixed chat
colour, random-map extras/clipboard, and the INI root/runtime Ares reader.

Fifteen files conflicted; the merge kept both sides' independent additions: the
ten i18n catalogs carry upstream's `VERA:RandomMap*` keys and the fork's
`VERA:Vmce*` keys; `lan_service.rs` keeps the fork's C0 setup-proof helper and
upstream's `chat_packet`/`chat_line`; `skirmish_session.rs` keeps the fork's
`cooperative_runtime_for_test` helper and adds upstream's `lan_name_opened`
field. `rules/ini_parser.rs` now follows upstream's runtime `ares_way()`
selection while retaining the fork's native preserve-empty external-config
parser and unconditional helper signatures. `sim/economy.rs` and
`sim/world/world_hash.rs` keep the mission `credits_earned` statistic beside
upstream's always-compiled, runtime-Ares storage field/hash. The world-command
tests pass the new `ares=false` argument. The shared-file VMCE gate pin drops
461 -> 459 (two ini_parser `cfg` seams became the runtime switch), then
459 -> 457 in the role-owner follow-up (one shared view gate and one shared
action gate moved to the VMCE-owned setup module); it stays enforced per file.
The diagnostics total, raised from 201 to 206 by the retry role probe before
this sync, is printed for observation and no longer pinned.

Validation on this tree: `check -p vera20k --lib --bins` passes with and
without `--features vmce`; architecture guards 52/52 in both configurations.
Focused tests: core `rules::ini_parser` 95 with vmce / 94 without, `sim::economy`
8/8, `sim::world::world_hash` 53/53, `sim::score` 15/13,
`sim::world::world_commands` 23/22; app `app::lan_service` 101 with vmce / 46
without, `app::frontend::skirmish_session` 81/80, `app::shell_network_setup`
50/33 (the role-owner tests), `app::match_runtime::session` 61/51,
`app::vmce_result` 26, `app::diagnostics::shell_capture` 104/88. The i18n
shipped-cameo test fails on this machine's retail copy both before and after the
merge (localized Vietnamese cameo frames), so it is not a sync regression. The
VMCE acceptance tool's own suite passes 37/37; the repository
`python -m tools.run_tests` keeps
the pre-existing cargo-cache symlink, macOS notarization, Windows path
separator and browser-switch path failures (upstream fixes the old
`<user-home>` failure).

Acceptance: the vmce-feature fast build `vmce-upstream-0f7dc1bbc-r1`
(executable SHA-256
`4EF6CE5F0C193A7880EEDD93AABD4F8CED9067A0817E9557BA5DD3D54A5E1C64`) passed
the two-process `2p-win` (`LDA-Continue-real`), `2p-retry` (`LDA-Retry`),
`2p-final` (`LDA-2p-final` stage02) and four-process `4p-win` (`LDA-4p`) runs.
The win/final runs matched 300 native checksums and 20 simulation hashes per
peer. The retry receipt matched the same 300-checksum / 20-hash prefix recorded
at the defeat page, before the reload; both peers then reloaded the same
`retry_mission` map and seed at native frame 0. The win runs reported `rows=2` /
`rows=4` and their expected next-stage or campaign-end receipts; the retry kept
its mirrored roles and defeat statistics; the final run returned both peers to
the main menu. Evidence lives in `.scratch/vmce-upstream-0f7dc1bbc-r11/`,
`-r12/`, `-r14/` and `-r15/` (not committed). The acceptance-tool wrap-up
renamed the retry checks to `pre-retry ...`, requires the pre-retry simulation
hashes and defeat statistics rows, and adds no claim of a post-retry frame
stream.

Role-decision ownership: shared `view.rs`, `actions.rs` and `service.rs` now
pass VMCE's role availability, guest-own-row rule, peer role admission and
map-change reset to the VMCE-owned `shell_network_setup/vmce_setup.rs`.
`MatchSession` remains the authoritative role/roster state owner; the shared
room, options transport, Ready invalidation and network queue are unchanged.
The fast build `vmce-role-owner-r1` (executable SHA-256
`145B42BDE2FB8295ED48BFC31FEDBEBAF27FFBD95A8E1778DD37F42B75D17DA7`) passed the
real two-process `2p-retry` run with mirrored authored roles
`[('LiveHost', 0, 1), ('LiveGuest', 1, 0)]` (not seat order), frozen Houses
`LiveHost=1` / `LiveGuest=0`, the same `retry_mission` map/seed on both peers,
300 equal pre-retry checksums and 20 equal pre-retry simulation hashes, and
identical defeat statistics. Evidence is in `.scratch/vmce-role-owner-r1/`
(not committed).

Residuals: this was a large upstream merge (860 files), but the conflict set and
validation concentrate on the shared lobby/LAN/INI/economy/hash seams. No
20,000-unit performance comparison or VMCE+Ares combination was run; WOL,
mixed-version, cross-machine, three-player and real OS-level packet-loss cases
remain outside this sync. The fast build's checksums are equal between its
peers, not across older builds. The new `arboard` dependency was fetched once;
subsequent Cargo runs use the cache.

## What the 0.3.2 merge changed (previous baseline)

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

## Previous upstream sync (2026-11-13)

Merged upstream main `867ae2467` (59 commits after 0.3.2) as `89ccbea49`; the
per-file conflict resolutions are in that merge message. Validation on the
merged tree: `check -p vera20k --lib --bins` with and without `--features
vmce`, architecture guards 26/26 in both configurations, focused VMCE tests
224, stock Cooperative/setup/LAN default tests 146, and a labelled VMCE build
(`vmce-upstream-867ae2467-r1`, executable SHA-256
`160163127F0FA58186456F88C06188113DA721BDB5F77295EA84AF5A51179749`) passing the
real two-process `2p-win`, `2p-retry` and `2p-final` acceptance (300 matching
checksums each; start, result, retry, intermission continue and campaign end).
The retry's 300 checksums in these older records are the prefix recorded at the
defeat page, before the reload; the post-reload identity is `retry_mission`
(map/seed/native frame 0), and the receipt has no post-retry frame stream.
The 20,000-unit performance comparison and the VMCE+Ares combination were not
run. The 2026-11-13 baseline kept VMCE as a compile-time `vmce` capability. That
implementation decision is superseded by the 2026-11-14 migration target: remove
game-behavior compile-time gates, keep VMCE on its independent branch and decide
runtime activation from evidence. The first entry-to-start slice now has a candidate implementation;
the remainder is still compile-gated. See the
[roadmap](zh-CN/vmce-roadmap.md#取消-vmce-游戏行为的编译期门控).

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

## Phase 3 first slice — VMCE settlement statistics ownership (feature/vmce, 2026-11-13)

VMCE's victory, defeat and campaign-end statistics are now decided by VMCE's own
role table instead of the stock Cooperative score-row selection. `sim/score.rs`
keeps one shared read-only `HouseScoreFacts` projection
(`Simulation::house_score_facts(owner)`) over the existing
`MatchStatistics`/`Economy`/`HouseTracking` counters; it projects one House and
selects no rows. `cooperative_score_rows` selects only human Houses for the
stock Cooperative result page and has no VMCE/mode branch. VMCE's owner is
`app/vmce_result/statistics.rs`: one row per admitted non-observer player from
the frozen `VmceHouseAssignments`, authored role metadata and `raw_slot_index`,
with the role House's shared facts; a House granted in addition to a primary
("shared House") is not a row, and a House named twice is counted once for the
kill total and the round participant count. The rows are projected once per
page and reused for the score model, the row count and the page values. The
`0x103` campaign score model moved to the neutral
`app/campaign_score_model.rs`, shared by the offline campaign exit and VMCE's
page; the dead duplicate VMCE model in `app/match_runtime/sim_tick.rs` and its
duplicate builder helpers were removed. VMCE victory/defeat diagnostics use the
VMCE projection and the generic Houses order/terminal snapshot, never
`cooperative_score_rows`.

Unchanged: result dispatch, round state, terminal-score latching, Scenario RNG,
defeat-page live statistics, and the victory page's layout/interaction. VMCE
still shares `shares_cooperative_result`, so its terminal snapshot is never
latched and victory statistics keep falling back to live facts (old behavior).

Architecture: `cooperative_score_rows` was removed from the VMCE
shared-capability allowlist and reference ceiling; a negative fixture now rejects
VMCE statistics calling it, and a new semantic guard asserts that (a) the
Cooperative selection has no VMCE/mode/outcome-viewpoint branch, (b)
`house_score_facts` names no mode and selects no rows, and (c) VMCE's statistics
owner reads its own assignment table and shared facts while no file under
`app/vmce_result/` reads the Cooperative selection.

Evidence (local): focused tests — core `sim::score` 13 (default) / 15 with
`vmce` (the extra one proves the Cooperative selection stays human-House based
inside a VMCE grant world), plus the restored empty-grant viewpoint coverage in
`house_defeat`; new VMCE statistics 7 (6 added: multi-role seat attribution,
shared House excluded, repeated House de-duplicated, passive row zeros,
terminal-first values, observers skipped; 1 moved time-header test);
`app::vmce_result` 25; `app::lan_service::vmce_round::tests` 20;
`app::lan_service::tests` 29; stock Cooperative session round 7; architecture
guards 45 with and without `vmce`; `check -p vera20k --lib --bins` passes both
configurations. Labelled fast build `vmce-stats-570af8d90-r2`, executable
SHA-256 `4407334D567700848E04B9EF88FE30201B5778F928E212BC50EE793DCD6D555C`:
`2p-win`, `2p-retry` and `2p-final` LAN acceptance all PASS under
`.scratch/vmce-stats-accept-r2/` with 300 matching native checksums and 20 matching
simulation hashes per peer, the expected next-stage/same-stage-retry/campaign-end
receipts, and victory receipts with `dialog.victory=true, rows=2` on both peers.
On the retry scenario both receipts' `vmce_statistics_rows` are identical; on the
victory and campaign-end scenarios each peer's diagnostics validate its
statistics rows against its own role House facts, and the 20 simulation hashes
are equal.

Residual: A19/B71 (passive primary House and the round's participant count) is
unchanged — the role-count projection still filters passive Houses, and the
user decided not to fix it; shared-owner statistics stay as they are. Three- and
four-player real-process settlement statistics were not run; multi-role
attribution and de-duplication are covered by the VMCE client tests only. The
terminal statistics source remains latent (VMCE never latches a terminal
snapshot, as before). Cross-machine, internet and OS-level packet loss stay out
of this slice.

## Phase 3 second slice — LAN lobby mode-event dispatch and C0 ownership (feature/vmce, 2026-11-13..14)

The shared lobby no longer routes VMCE's join/leave and related mode events
through Cooperative's handlers. `app/shell_network_setup/dispatch.rs` owns the
native mode choice: a join accept (`0x005E2FA0`) and accepted departure
(`0x005E11DC`) read Choose Map's selected game type while the chooser is on top,
else the admitted room's mode; a chooser row that names no game type (VERA's
Favorites/Online Maps rows) selects no mode and does not fall back to the room.
Setup open (BC `0x005ED6FE..0x005ED717`), the join's
`+0x1C`/`+0x28` slot (`0x005E1945`), every options-parse `+24` refresh, mode
entry (`0x005E541D`), Use Map (`0x005E7160`) and a guest's choice-country
admission read the admitted room's mode. The dispatch hands each event to
Cooperative's own slots in `cooperative.rs` or to VMCE's slots in
`app/shell_network_setup/vmce_setup.rs`; Cooperative's handlers lost their own
mode guards so the dispatch table is the single author of the decision. VMCE's
join slot owns the playing-seat Random role default that used to sit inline in
the shared service. A non-`vmce` build uses `vmce_setup_stub.rs`, where no VMCE
mode exists and every slot is a no-op. The VMCE-config dispatcher tests call the
production `live_owner_for` predicate table; the non-VMCE config keeps a
would-be extension predicate because its production stub claims no mode.

A received Cooperative campaign text (`C0`) is not a mode-dispatched event, and
an unconditional forward is not its ownership either. Native's `'C'` arm
(`0x005E43CE`) dispatches every mode object's `+0x58`, and only MPCooperative
takes it. `app/shell_network_setup/lobby_campaign.rs` resolves the owning setup
from the message itself:

- WOL carries `C0` inside the host-options payload (`'L'`, `0x5E8EA0`), so the
  payload's `launch.mode` owns it. Only a Cooperative payload's text is applied,
  ahead of the same payload's `HostSetup` because the native host publishes it
  first; a non-Cooperative payload never reaches Cooperative's state. The
  existing WOL revision check still discards older options events.
- LAN sends `C0` as its own short options packet (`0x17`); the session proof's
  `scope` (epoch + revision) names the options revision it followed. A text
  from another admission, or below the revision the current setup replaced, is
  discarded. A text that overtakes its `HostSetup`, or arrives while this guest
  still holds the old non-Cooperative mode, is held in a bounded in-memory
  inbox. The next `HostSetup` resolves it: a Cooperative setup applies texts at
  or after the revision it replaced (so a delayed transition text still
  applies), a non-Cooperative setup discards them, and later revisions stay
  held. The inbox is cleared on room leave and on a non-Cooperative mode
  commit; the Cooperative
  publisher queues `C0` only in an admitted Cooperative room, and the LAN/WOL
  send queues re-check the room at dequeue so a stale text cannot borrow the
  next options revision's proof.

The application itself still runs through the Cooperative progress owner's
`accept_received_campaign` (`receive_campaign` + `receive_stage`), with no second
campaign state. The held text is resolved after the complete HostSetup
transaction is accepted and before the mode entry refreshes the setup, so the
early and normal orders both end with the host's campaign, stage and stage
countries.

Unchanged: room/host/roster/admission/network/reliable queue stay shared and in
VERA's owners; host and guest join, leave, map choice, Ready and Start behavior
(including the map chooser's selected-mode semantics during a join/leave — own
rows such as Favorites/Online Maps run no mode slot — async result invalidation,
launch revalidation and deterministic random roles) is as before; ordinary
Cooperative events behave as before. This is a dispatch split, not a mode plugin
framework.

Architecture: `lobby_mode_events_dispatch_before_the_cooperative_owner` asserts
that shared setup files and the service layer never call the Cooperative mode
module, the VMCE lobby module never reaches it, the root adapter routes its
event calls through the dispatcher, and the Cooperative handlers carry no mode
test of their own. The C0 checks are now the real boundary rather than an
unconditional-forward pin: `deliver` must handle `CampaignOrigin::Options` and
`CampaignOrigin::Lan`, compare scope/revision, hold an early text and use
`campaign_may_apply`; `options_applied` must take the hold, observe the new
revision and apply only in a Cooperative setup; the WOL decoder must attach the
payload's mode and the LAN receiver its proof scope; Cooperative's `publish` and
the LAN/WOL send queues must gate on the admitted Cooperative room. Planted
negative fixtures reject an unconditional forward, a forward gated only on the
current room mode, a dropped hold and an unconditional publisher. The
dependency guard's shared-owner table records `dispatch::host_opened`/`map_used`
as the seam VMCE code reaches through the shell host-setup open/return paths and
the capture fixture's Use Map commit; the shared-mode reference pin for
`service_event` stays 1.

Evidence (local): `lobby_campaign` 9 production-handler tests (an ordinary mode
switching into Cooperative with `C0` before the HostSetup reaches the host
campaign/stage/countries; WOL same-payload early order; normal order; reordered
late order with a distinct stale text; a non-Cooperative room never applies a
campaign; Cooperative → other mode → Cooperative again, including the old
room's text after the return; room-leave clearing; queued-publication clearing;
the same options revision re-applied for a roster keeps the floor) on real
`MatchSession`, `MultiplayerShellRuntime`, `CooperativeProgressState` and wire
event identities, not direct Cooperative state calls; LAN receive scope 1; WOL
payload ownership 1. The wrong-implementation checks fail as intended: removing
the options owner check fails the non-Cooperative test, replacing the LAN
scope/hold with the old current-room-mode filter fails the early,
non-Cooperative and leave tests, disabling the delayed-text revision floor
fails the reordered and stale-after-return assertions, and making the floor
advance on every re-application fails the repeated-setup test.
`app::shell_network_setup` 47 with `vmce` and 33 without;
`app::shell_wol::live_service` 55; `app::lan_service` 98 (vmce) / 42 (default);
`app::frontend::skirmish_session::cooperative::tests` 30; architecture guards 46
both configs; `check -p vera20k --lib --bins` both configs. Labelled fast build
`vmce-lobby-c0-c33dc84be-r2`, executable SHA-256
`1038e55c50d426a93664fdb9f251f655b4f246a660f48985dc61fab662ae139d`: `2p-win`
LAN acceptance PASS under `.scratch/vmce-lobby-c0-accept-r2/` with 300 matching
native checksums, 20 matching simulation hashes, `dialog.rows=2` and the
expected next-stage receipt.

Residual: no complete `AppState` fixture exists, so the `service_event` →
`dispatch` → HostSetup transaction itself has no unit test; the tests drive the
same production handler with real owners and the wire/event constructors, and
the guard pins those call sites. In particular the production `AppCampaignVisit`
mapping (protocol inbox ↔ `lan_service.setup_scope()` ↔ the admitted mode) is
not executed by a test. The HostSetup hook resolves the hold before the
remaining mode-entry steps; if a later setup step aborts, the session/scope is
restored but not the inbox floor or the already-applied campaign (the native C0
application was always independent of setup success), which is a narrow
follow-up rather than a merge blocker. The online (WOL/relay) early-`C0` order
and a live stock-Cooperative LAN join/leave were not reproduced on a real
client, and the acceptance run uses the VMCE series, which carries no
Cooperative `C0`, so it does not validate this fix in a live Cooperative room.
Mid-room leave, 3/4-player lobbies, cross-machine and internet play remain
unrun. Per the user's decision: B59, B71 and B72 stay user-accepted residuals
(not fixed, no extra fallback); shared-owner statistics stay as they are; a
normal player's wait must not be misjudged as a timeout remains a pending
problem, moved out of the not-to-fix area and not claimed fixed by this slice.


## Phase 3 third slice — VMCE entry-to-start admission (candidate, 2026-11-14)

The candidate removes compile-time gates from the stable VMCE mode ID, `VmceSeries` map-source
filter, and guest options parser. Runtime availability is not inferred from row presence: it is
the validated VMCE campaign entry set already owned by `ScenarioCatalog`. Host map visibility and
the GO path both read that owner. A feature-off migration bridge keeps the row hidden and rejects
VMCE options/host setup before shared session state changes; a feature-on build keeps the existing
map/config/role/seed and LAN checks. No wire format changed.

Focused validation passed for feature-off production `check`, feature wasm check, the VMCE gate
budget guard, identity registration, guest parsing/backend refusal, chooser visibility, VMCE LAN
prevalidation, feature-on start role resolution, and 30 stock Cooperative direct-consumer tests.
The candidate after critic fixes passed a two-process LAN win and next-mission run outside the
Windows sandbox, with 300 matching checksums and 20 matching simulation hashes. The sandboxed
executable failed with `0xC0000142`; the same executable starts outside the sandbox. The host
synthetic lobby capture reports an unclaimed role in both this candidate and the previous saved
build. Feature-off guest admission, WOL, old released clients,
mixed LAN builds, cross-machine play, and the wider compile-gate removal remain unverified.

## Phase 3 fourth slice — shared lobby difficulty ownership (feature/vmce, 2026-11-14)

The lobby difficulty row is native process state `[0xA8EB68]`, initialized to zero and written by
the shared LAN/WOL setup dialog trackbar `0x732` (0..2); Cooperative reads it for its BC6 rounds
and the `0xBC7` campaign-end label, VMCE uses it for its lobby trigger row, and the LAN options
payload carries it in its own field named `cooperative_difficulty`. The slice therefore treats it
as a shared lobby fact, not Cooperative-owned state: `skirmish_launch::LobbyDifficulty` types the
row, `MultiplayerShellRuntime` is its single owner, the LAN decoder normalizes once at the receive
boundary, and both the native LAN field name and the WOL JSON field name stay unchanged. VMCE's
old inline freeze closure moved into `vmce_setup::frozen_start_houses`, which reads the owner once
at start commit into `VmceHouseAssignments.difficulty`; the loader's TriggerType flags and the
result page read that frozen copy, and the result page falls back to the NORMAL row only when no
assignments exist. Cooperative keeps reading the same owner row; its behavior is unchanged.

Guards: the shared-capability allowlist entry and owner mapping for `cooperative_difficulty` are
removed; the VMCE lobby-owned files join the existing whole-file dependency scan, so they are
rejected for direct Cooperative lobby-owner calls and for resolvable indirect wrapper calls, with
planted direct and indirect negative fixtures; a new semantic guard pins the result model to the
frozen start copy and the live row out of `result_difficulty`; a second pins `frozen_start_houses`
to the shared `lobby_difficulty()` owner and the shared `commit_start` to that VMCE entry; the
pinned mode-check inventory drops the `commit_start` caller now that admission decision lives in
the VMCE-owned helper. All 51 guards pass in both configurations.

Tests: `app::lan_service::options` vmce 23 / default 13 (including a new out-of-range wire-row
normalization test and session `accept_network_host_setup` apply assertions);
`app::frontend::skirmish_session` vmce 79 / default 78 (new normalized-row test);
`app::shell_network_setup` vmce 47 / default 33; `app::vmce_result` 26 (frozen-difficulty result
test); `app::cooperative_round` compiles in both configurations but has no unit tests;
`check -p vera20k --lib --bins` passes in both configurations.

Acceptance: the vmce-feature fast build `vmce-lobby-difficulty-r3` (exe SHA-256
`6A578BAD51B1CF38A723278E965B8EB9805C76F05874B91036DAC199A4B50D63`) passed the two-process
`2p-win` lobby join, settings, ready, start and next-mission run with the `LDA-Continue-real`
series: both ends matched 300 native checksums and 20 simulation hashes, the victory page receipt
reported `rows=2` and `can_continue=true`, and both ends reported the stage02 next mission.
Evidence lives in `.scratch/vmce-lobby-difficulty-accept-r2/` (not committed). A default-feature
build lacks the `lan-live-vera-campaign-*` checkpoints and cannot run this acceptance.

Residuals: no constructible full `AppState` fixture, so the slider write → freeze closure →
`frozen_start_houses` glue is not executed by a unit test (same production function, shared owner,
LAN decode boundary and result consumer are covered, call sites pinned by guards); the acceptance
series does not move the difficulty slider, so the runtime slider choice is not proven end to end;
WOL, older clients, mixed builds and cross-machine play are unverified. Decode-time normalization
of out-of-range rows is defensive boundary handling, not a claim about native behavior (native only
accepts the 0..2 trackbar input); negative rows defensively clamp to Easy, while native
Cooperative5C31A5 maps any other row to Hard, so the two differ only for rows neither the trackbar
nor a VERA encoder authors. The WOL `Options::into_options` path reuses the same normalization
constructor but has no WOL-side mirror test. The result page's NORMAL fallback for a missing
assignment is a defensive default (a real VMCE result page always has the frozen assignments; the
old live-row read usually returned 0 there, so the difference is unreachable).

## Phase 3 fifth slice — room team-seating policy ownership (feature/vmce, 2026-11-14)

`MatchSession::network_team_room` decided whether a VERA room groups its players into team seats
by asking `!launch.mode.class.uses_cooperative_setup()` — a shared room question phrased as a
Cooperative question — and the setup view repeated the exclusion with
`!mode_policy.authored_campaign_roles`. The slice moves the decision to the mode's own policy
owner: `ModePolicy::room_teams` is true for Battle, ManBattle, Siege, Unholy and FreeForAll and
false for Cooperative and Vmce. `network_team_room` now combines only genuinely shared room facts
(a Setup phase with several observers, `allies_allowed`, not a clan room) with that policy field,
and the setup view reads the same owner without the duplicate mode check. Visible behavior within a
room is unchanged: stock rooms still group rows and expose TEAMS and the team controls; Cooperative
and VMCE rooms still do not (the mode-switch round trip is recorded under residuals). VMCE's
`allies_allowed: true` row no longer needs a separate exclusion, and the shared room predicate
retains no Cooperative question.

Guards: the mode policy stays exhaustively constructed, so every class states its value, and the
per-mode policy literal test includes the new field. The diff adds no `is_vmce` call, so no
shared-capability allowlist or mode-check ceiling entry changed. All 51 guards pass in both
configurations.

Tests: core `skirmish_modes` 13; `app::match_runtime::session` default 50 / vmce 60, including the
new `room_teams_follow_the_mode_policy`, which drives the real
`MatchSession::enter_network` + `accept_network_setup` for Battle, Cooperative and Vmce with a
stale `options.teams: Some(2)` left by a mode switch, asserts `network_team_room` and
`network_teams()` agree, covers `allies_allowed = false`, and was verified to fail with the old
predicate (Vmce returned true); `app::shell_network_setup` vmce 47 / default 33; `check -p
vera20k --lib --bins` passes in both configurations.

Acceptance: the vmce-feature fast build `vmce-team-seating-r1` (exe SHA-256
`D9929F3E95CA2DE47D9C9B5A4DE8CAD07CC5E6E3E76F7CE298363C005EB5D981`) passed the two-process
`2p-win` lobby join, settings, ready, start and next-mission run with the `LDA-Continue-real`
series: both ends matched 300 native checksums and 20 simulation hashes, the victory page receipt
reported `rows=2` and `can_continue=true`, and both ends reported the stage02 next mission.
Evidence lives in `.scratch/vmce-team-seating-accept-r1/` (not committed).

Residuals: this slice is a behavior-preserving ownership migration, not a visible fix; the
acceptance run proves the VMCE lobby/start chain still works rather than that a control changed
(the earlier view check already suppressed grouping for VMCE and Cooperative). One edge state can
differ: a stock-mode host who sets a team count and then switches into VMCE through the Choose Map
window. The old predicate treated that stale count as team seating inside the VMCE room
(`keep_players_on_teams`, auto-balance, preserved `slot.team`, WOL Balance); the new policy does
not. VMCE's own interface is identical either way (TEAMS disabled, rows ungrouped); the only
visible difference is a round trip back to the stock mode, where the old code kept the pre-VMCE
layout and the new code re-deals rows. That switch path was not exercised in this slice's
production acceptance. A later regression test through the production map-selection owner now verifies the preserved layout; a live room round trip remains unverified. WOL Balance, spectator rooms and clan-room team paths keep their existing
tests but were not separately exercised in production. Three/four-player rooms, cross-machine play
and the internet path remain outside this slice.

## Phase 3 sixth slice — VMCE-owned start permission (feature/vmce, 2026-11-14)

A VMCE room's start permit (runtime mode availability, selected map, LAN-only config sync, the
synchronized map config, the accepted shared seed and the map-authored role resolution) and its
minimum-player rule (at least one non-observer player) were inlined in VERA's shared start command:
`mode_start` in `src/app/shell_network_setup/actions/start.rs` carried a `#[cfg(feature =
"vmce")]` branch with the whole VMCE validation, the player-facing denial strings and the map/role
logs, and `has_minimum_players` carried the mode rule. The slice moves both into the VMCE-owned lobby
module `src/app/shell_network_setup/vmce_setup.rs` (`VmceStartPermission`, `start_permission`,
`minimum_players_met`). The shared command keeps one `is_vmce()` dispatch per function, the native
command ordering, readiness/publishing and the GO freeze, and prints the owner's reason. The
feature-off migration bridge (`vmce_setup_stub.rs`) mirrors the same symbols and keeps refusing a
VMCE start with the "no playable VMCE backend" reason, so no feature-off path reaches an incomplete
backend. Stock and Cooperative start paths are untouched; the removed cfg gate required the pinned
budget edits (`actions/start.rs` 2 -> 1, `VMCE_PRODUCTION_GATE_TOTAL` 462 -> 461).

Guards: the new `vmce_start_permission_is_owned_by_the_vmce_lobby_module` scans raw function bodies
and asserts the shared `mode_start` dispatches `vmce_setup::start_permission` while carrying none of
`resolve_vmce_houses_with_config`, `network_vmce_map_config`, `load_network_map_ini`,
`vmce_map::mode_available` or the deny text; that `has_minimum_players` dispatches
`minimum_players_met`; that the VMCE owner holds those rules; and that the migration bridge keeps
its slots and refusal. All 52 guards pass in both configurations.

Tests: `app::shell_network_setup` vmce 48 / default 33, including the new
`the_vmce_lobby_requires_a_player_and_never_counts_an_observer_as_one` (through the VMCE owner:
one player passes, an observer or an empty list does not) and the existing VMCE solo-host minimum
tests that now dispatch through the production shared command. `check -p vera20k --lib --bins`
passes in both configurations.

Acceptance: the vmce-feature fast build `vmce-start-permission-r1` (exe SHA-256
`90D71AFD07CC5CD187C88A93FB8A737F7B105AF32C0FE326FC560837E41DC99B`) passed the two-process
`2p-win` lobby join, settings, ready, start and next-mission run with the `LDA-Continue-real`
series: both ends matched 300 native checksums and 20 simulation hashes, the victory page receipt
reported `rows=2` and `can_continue=true`, and both ends reported the stage02 next mission.
Evidence lives in `.scratch/vmce-start-permission-accept-r1/` (not committed).

Residuals: this is a behavior-preserving ownership migration; the acceptance run exercises the
allowed branch, not the denial branches (no backend, no selected map, non-LAN, missing map config,
missing seed, invalid roles), which need a full `AppState` and are covered only by the static guard
for dispatch and text ownership. The migration bridge keeps the mode's own minimum-player
expression rather than a fixed false, so a populated feature-off room would still reach the
backend-refusal reason; a feature-off VMCE room is itself unreachable today, so that dialog was not
exercised in the game. WOL, old/mixed builds, cross-machine play, three/four-player rooms and the
internet path remain outside this slice.

Related pre-existing guard gaps (not fixed here): three pinned gate budgets are stale at a zero
count (`app/cooperative_round.rs` 12, `app/lan_service/cooperative.rs` 19,
`app/match_runtime/session/cooperative_round.rs` 37) and the budget guard skips zero-count files, so
the table sum does not equal `VMCE_PRODUCTION_GATE_TOTAL`; `resolve_vmce_houses` in
`actions/start.rs` is a test-only `#[cfg(feature = "vmce")]` shared-file gate.

## Phase 3 seventh slice — lobby acceptance probe: claimed seats and difficulty (feature/vmce, 2026-11-14)

The live two-process VMCE acceptance used to accept the host's options as delivered: every participant
started on Random roles and the lobby difficulty stayed at its process default, so the role and
difficulty parts of the lobby chain were covered only by unit and architecture checks. The acceptance
tool now drives those inputs through production code before Accept/Start:

- `--lan-lobby-inputs` (VMCE live LAN host or guest only) enables the probe. Both peers claim the
  authored primary role equal to their admitted seat row through the setup Team column (the VMCE role
  slot); the host sets the shared lobby difficulty to Hard (row 2) through the production difficulty
  action handler; pointer hit-testing and dragging are not exercised. The guest re-sends production Accept whenever a later material host-options revision clears
  its Ready row, so the probe follows what a real guest does instead of racing one options revision.
- `vmce_setup_snapshot`, `vmce_runtime_receipt` and `vmce_lobby_receipt` carry `lobby_difficulty`, the
  frozen start `frozen_difficulty` (from the frozen `VmceHouseAssignments`), `role_claims_enabled` /
  `roles_claimed`, and per-seat roles. The Ready transition also pins the lobby row and, when claims
  are enabled, requires each seat to own its row's role.
- The `2p-win` scenario requests the probe; the comparator adds three checks: every seat claimed its
  own authored role and both receipts agree on the name/row/role mapping; the lobby difficulty is
  equal across peers and equals the requested row; the frozen start difficulty equals it as well.
- `setup::apply_action(SetupAction::Accept)` is used for the probe instead of pointer hit-testing: the
  click path did not reach the production Accept after the setup actions in this synthetic frame
  sequence, while the action itself is the same production handler the button invokes.

Tests: `app::diagnostics::shell_capture` default 88 / vmce 100; `check -p vera20k --lib --bins` passes
in both configurations; the acceptance tool's own unit suite has 33 passing tests (new comparator and
command-line probe cases). The repository-level `python -m tools.run_tests` still reports its
pre-existing environmental/branch failures (cargo-cache symlink tests, macOS notarization, the
existing `<user-home>` literal in `src/app/restart.rs`, the browser-switch path list), none of which
touch the three files changed here.

Acceptance: the vmce-feature fast build `vmce-lobby-inputs-r2` (exe SHA-256
`D99162C7438EFE0BD4ADE123C639F281B1AF2772141944A89716FF6634EFA788`) passed the two-process
`2p-win` run with the `LDA-Continue-real` series: both ends matched 300 native checksums and 20
simulation hashes, the victory page reported `rows=2` / `can_continue=true`, the stage02 next mission
was present on both ends, both receipts reported lobby and frozen difficulty 2, and the role probe
reported `[('LiveHost', 0, 0), ('LiveGuest', 1, 1)]`. Evidence lives in
`.scratch/vmce-lobby-inputs-accept-r9/` (not committed).

Residuals: only `2p-win` enables the probe; `2p-retry`, `2p-final`, `4p-win` and the stock Cooperative
captures keep their previous scripts (the driver is VMCE-only and the comparator checks run only when a
scenario requests them). The probe proves the normal host/guest allowed path; it does not exercise a
rejected role/config change, leave/reselect in the intermission room, WOL, old/mixed builds,
cross-machine play or three/four-player rooms. No game behavior changed: the work is in
`src/app/diagnostics/shell_capture.rs`, `.../movies/lan/live.rs` and
`tools/vmce_lan_acceptance.py`.

## Acceptance repair — deterministic retry role/score-order probe (feature/vmce, 2026-11-14)

The `2p-retry` live capture asserts that VMCE's defeat statistics follow the frozen role/House
assignment rather than the map's House storage order: the diagnostic requires the admitted seat order
to differ from the authored House order before it validates each row's House, colour, role facts and
score values. The scenario previously started every seat on Random, so that precondition depended on
the seed's draw; a run whose draw kept the identity mapping failed at `retry probe did not reverse the
actual score and seat order` after both peers had reached the defeat page. This was a test-precondition
problem, not a game defect: the lobby role, freeze, retry, result and score owners were unchanged.

Fix (acceptance code only): the retry scenario now requests the existing lobby role probe
(`--lan-lobby-inputs`), and each seat claims the mirrored authored role (`role_count - 1 - row`)
through the production Team-column `SetupAction::Select`. The host publishes the choice in its normal
options revision; the guest sends its normal `PlayerChoices` over real LAN. This covers the production
action handler, not a real mouse click, and it injects no frozen assignment, result, network message or
per-peer state. `2p-win` keeps the seat-identity mapping; `2p-final`, `4p-win`, `a03-*` and the
Cooperative captures do not enable the probe and keep random roles. `vmce_setup_snapshot` records
`role_claims_reversed` and `probe_role_count`, and the Ready transition requires the probe's exact role
for every seat when claims are required; `vmce_runtime_receipt` also records the authored
`role_count`. The comparator now requires the scenario's exact mapping (identity or mirrored) with the
authored role count, so a random draw can no longer pass as a claim.

Tests: the focused `app::diagnostics::shell_capture::movies::lan::live::vmce_ready_transition_tests` (4)
pass with `vmce`; the acceptance tool's unit suite now has 37 passing tests (three retry-prefix and
statistics cases were added by the follow-up wrap-up). The new mirror/Ready-transition
assertions and the new Python comparator case were each run once against the pre-fix identity
behavior in the working tree (temporarily reverted, not committed): the two Rust tests failed, and the
identity-only comparator rejected the mirrored receipts; both pass with the fix. `check -p vera20k
--lib --bins` passes without `vmce` (the Ready-transition helper is VMCE-only and now gated). The
repository-level `python -m tools.run_tests` still reports the environmental/branch failures noted
earlier (cargo-cache symlink tests, macOS notarization, the `<user-home>` literal in
`src/app/restart.rs`, the browser-switch path list and the Windows `<path>` separator); none touch the
files changed here.

Acceptance (this round): the vmce-feature fast build `vmce-retry-probe-r3` (exe SHA-256
`B139A9E633F03B119D969AB35B2BF3C3DCCE63F9AECB837B4ACE3E3919E271F4`) passed the real two-process
`2p-retry` run with the `LDA-Retry` series. Both peers reached the defeat page (`victory=false`,
`rows=2`, equal `state_hash`), reported the same mirrored seats `[('LiveHost', 0, 1), ('LiveGuest', 1,
0)]` with `role_count=2` and lobby/frozen difficulty 2, then readied and retried. The receipt's
pre-retry prefix had 300 equal native checksums and 20 equal simulation hashes; both peers then
reloaded `VMCE/LDA-Retry/stage01/all01umd.map` with the same seed at native frame 0, and the defeat
statistics rows were identical with non-zero House facts. The same build passed `2p-win` (identity
probe, `LDA-Continue-real`) and `4p-win` (probe off, four random `-2` roles and `role_count=4`,
`LDA-4p`). Evidence lives in
`.scratch/vmce-retry-probe-accept-r8/`, `-r9/` and `-r10/` (not committed).

Residuals: the retry probe covers the allowed production role/difficulty path only; real mouse
operation, rejected role choices, leaving/reselecting roles in the intermission room, WOL/mixed builds,
cross-machine play and three-player rooms were not run. The comparator checks the peers' pre-retry
checksum/hash prefix and defeat statistics rows; the receipt has no post-retry frame stream, so no
post-retry hash claim is made. It does not add a separate cross-peer `state_hash` assertion, so that
equality was verified from the receipts for this round. Historical `2p-retry`
receipts made before this repair used random roles and may have crossed the same assertion only by
draw.
