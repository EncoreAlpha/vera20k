# VMCE backlog

Living task ledger for known VMCE gaps. Severity: S1 blocks the next release, S2 is a
visible correctness/claim gap, S3 is polish/robustness. Evidence for each item is in
`.scratch/vmce-logic-audit/` (local audit reports, not committed) and the cited source.

## S1 — release blockers

- **B01 Retry never restarts.** Retry freeze restores `network.vmce_map_config` but the rebuilt
  `PreparedNetworkStartup` lacks `.with_vmce_map_config`/`.with_local_map_source`; loader aborts
  with "frozen VMCE startup has no map config" and both peers return to the lobby. Fix
  `freeze_cooperative_round` retry branch, add a startup-identity test and a real 2p-retry
  capture. Evidence: result-round.md, sim-outcome.md; `session/cooperative_round.rs`.
- **B02 15 s retry grace / 30 s campaign-end pull are captions only.** No timer, no guest
  page-entered state; retry host starts immediately; campaign end keeps `all_ready()` forever
  while 0x103 has only Continue. Implement guest-entered event, host deadline pump, forced
  campaign-end start, late-guest lobby; tests for all three continuations. Evidence:
  result-round.md, contract.md; `cooperative_round.rs`, `vmce_result.rs`.
- **B03 Setup apply is not atomic.** `receive_extended` commits scope/agreement/options before
  apply; failed apply is debug-only and unrecoverable. Guest can accept GO then fail freeze with
  "local slot still has a random color". Validate/apply before commit or roll back; warn log +
  regression test. Evidence: net-lobby.md; `lan_service/options.rs`, `scenario_bootstrap.rs`.

## S2 — correctness / claims

- **B04 Defeat/last page paints a ready gate it does not have.** Ready glyphs + "Ready / Host"
  header remain, `can_continue=true` on all VMCE pages, ready count discarded. Align projection
  with the real gate; correct `0x89E05D` comment and receipt meaning. Evidence: result-round.md.
- **B05 Strict campaign.ini validation covers only the Entry stage.** A malformed later stage
  plays and fails after the previous win, silently returning both peers to the lobby.
- **B06 Role auto-defeat side effects contradict docs.** Mid-mission role House defeat still
  fires EVA/full-map reveal/UI gating while the mission continues; decide native/desired
  semantics and fix or document.
- **B07 Docs/evidence overstate validation.** Final/retry/mismatch captures not recorded; some
  docs say implemented and unsupported in the same file. Correct both guides after behavior is
  fixed.
- **B08 i18n VMCE labels are English copies.** After the upstream-sync policy, en has real text,
  zh-CN keeps fork translations, other 8 languages use English placeholders; completeness tests
  only check presence. Decide translation pass or documented fallback.
- **B09 "Guest cannot Ready without series CSF" is false.** Identity hashes campaign.ini + map
  bytes only; fix claim or add the missing check.
- **B10 One-shot campaign-end/rejection records can be lost** after `release_transport`; dead
  peer during the round is not detected. Add retry/ack or keep transport until receipt.
- **B11 Next-stage identity caps catalog map path at 215 bytes** in the fixed LAN envelope;
  longer valid paths fail closed to the lobby.
- **B12 Observer seats reset to Random on map/mode change;** VMCE start rejects "observer has a
  player role" and the UI cannot restore Observer, so the room can be stuck.
- **B13 Guest role changes are not republished to 3rd/4th guests** on LAN (WOL path does).
- **B14 Invalid-campaign modal is near-unreachable** (discovery filters the series first) and
  keyed on localized string equality; lobby status classifier matches English substrings.
- **B15 Host does not re-read its own campaign.ini** between start publish and frozen commit
  (fails closed only at next stage).

## S3 — robustness / tests

- **B16 Setup apply failures are invisible** at default log level; freeze error misattributes all
  humans to "local slot".
- **B17 Missing tests/captures:** retry-freeze/load, 15 s/30 s timers, defeat projection,
  campaign-end; 2p-final/2p-retry real captures; 3-player path; RTL visual.
- **B18 Test workflow:** per-phase deadlines, short dev probe defaults, retail directory reuse.
  (Current: `--suite dev`, 120 s cap, fail-fast; phase deadlines and cache reuse still open.)
- **B19 Evidence baseline:** upstream-sync changed gameplay; old 2p/4p receipts are invalid.
  Keep new baseline in docs/vmce-baseline.md and do not compare against old runs.

## UI / deferred

- **B24 Lobby/result label and column revision (TODO, not implemented).** Approved copy:
  lobby becomes `玩家 | 所属方 | 国家 | 准备 | 颜色`; the interactive column header is
  `所属方` and its selected face shows only the map House name (no `P1..P4`, no role number);
  the separate `角色`/Role display column and its header are removed. The `所属方` value is
  the map `[Houses]` House bound by `campaign.ini PlayerN` (not Country/Side). `准备` replaces
  `就绪/主机`: host always shows the native H-in-red-star glyph; a guest is blank until ready
  and then shows the normal red star; the `主机` subcolumn/header is removed. The result page
  follows the same columns: `玩家` (lobby player name) | `所属方` (map House) | `国家` |
  `结果` | `准备`; on defeat only the host shows the H star and guests stay blank. Reuse the
  native H + red-star art (compose/align existing glyphs if there is no combined native
  icon); no new bitmap art. Remove the 119 px role column and redistribute widths by actual
  layout needs (no large blank area or overlap; right edge stays 632); the interactive combo
  keeps its rect and role-assignment behavior. i18n: en `Owner`/`Ready`, zh-CN `所属方`/`准备`;
  other 8 languages use English placeholders per the current policy; drop only keys that
  become unused. Acceptance: 800x600 lobby/result screenshots for host/guest/defeat plus one
  1920 shot; typography/inset rules from the existing lobby; no protocol/sim changes.
  Open items at implementation time: exact column widths, whether a combined H-star native
  icon exists, and the defeat-page `准备` header wording.
- **B20 Game HUD:** waiting/sync status, campaign identity (stage/seed/series), disconnect/failure
  notices; pause semantics stay native "menu does not freeze".
- **B21 Loading/transition:** campaign identity, stage/seed, 4-player readiness.
- **B22 Language policy:** VERA labels follow system locale while native CSF follows install
  language; decide default (install language vs system vs explicit setting).
- **B23 Remaining campaign keys:** `AltNextScenario`, `SkipMapSelect`, cross-mission money/role
  carry-over, defeat retry timers (B02) — all deliberately deferred in the current batch.

## Completed, keep out of the backlog

- Four-player roster/setup relay fix and real 4p-win acceptance.
- EndOfGame/defeat production skeleton, strict Entry-stage validation, invalid-campaign modal.
- Pause: native "menu does not freeze" verified and documented.
- VMCE lobby/result UI batch and upstream 0.2.9 sync.
