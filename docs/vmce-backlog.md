# VMCE backlog

This is the English engineering ledger. The [Chinese consolidated review](zh-CN/vmce-review.md) is
authoritative for priorities and currently holds 55 entries (45 `A` findings plus 10 B-only
residuals); this ledger keeps the same numbering and records unfinished work, locked decisions
and acceptance boundaries. The current baseline is upstream 0.3.2, merge `70d569b86`, fork
version `0.3.2-vmce.6`; the interactions the sync exposed are B101–B105 below
(B103 and B105 are closed by the merged-baseline acceptance; B101, B102 and B104 stay open). Implemented
paths appear under [Implementation and evidence limits](#implementation-and-evidence-limits); an
implemented path can still have an open defect. The [player and map-author guide](vmce-guide.md)
describes current behavior. The refactor-only B5 series and the B106 campaign-data owner are recorded below;
they change module ownership, not behavior. Planned or follow-up items are not implementation promises.

## Post-0.3.2 baseline interactions (B101–B105)

These are new work created by the upstream 0.3.2 merge, not re-numbered audit findings.

- **B101 — Vietnamese VMCE strings:** upstream added `src/i18n/vera20k.vi.ini`; it carries no
  `VERA:VMCE*` keys, while the ten older catalogs carry 32 each (English and zh-CN real, the other
  eight English placeholders). Add the 32 keys (English placeholders are acceptable) and keep the
  i18n completeness suite green.
- **B102 — rules and extension identity in series admission:** the VMCE admission digest covers
  `campaign.ini` and every manifest map's path/length/bytes only. Upstream now enables Ares from
  the game's data (`RuleSet::ares`), and mods can change rules; peers with different data can
  still admit the same series and then desync. Decide whether to include a rules/extension
  identity digest in admission or to refuse mixed data explicitly, and document the requirement.
- **B103 — 1024x768 default-window pages (closed 2026-11-12):** the launch window is now
  1024x768 (`VERA_LAUNCH_WINDOW_SIZE`) instead of 800x600; the VMCE `0x103`/`0xBC6` pages are
  authored at 800x600 and fitted by `ShellScale`. The merged-baseline run captured
  `vmce-result-host-victory-next`, `vmce-result-host-victory-stats` and
  `vmce-defeat-host-disabled-2p` at 1024x768; all three matched the B2 baseline at the same size
  (3/3, byte-identical frames and stable metadata). The VMCE lobby was not re-captured at
  1024x768; its layout is shared with the existing 800x600/1600x900 coverage.
- **B104 — Auto Repair and shared-House grants:** Auto Repair sends Repair orders to damaged
  buildings. VMCE's `PlayerNControl` grants let a player command shared Houses, but the repair and
  sell paths (B70) and self-click/deploy actions (B75) already miss some grant checks. Cover the
  Auto Repair path with the same grant (or refuse it consistently) and add a focused test.
- **B105 — simulation-thread result-page reads (closed 2026-11-12):** desktop frames tick on
  the worker by default, and `SimHost::runtime()` returns the published presentation frame while
  the runtime is lent. The merged-baseline LAN run (`2p-win`, `2p-retry`, `4p-win`, `2p-final`;
  93/93 checks, 300 matching checksums each) rendered the live victory board and the live defeat
  statistics page non-empty, with equal host/guest result fields and state hashes; the result
  pages were also pixel-identical to the B2 baseline at 800x600, 1024x768 and 1600x900. No empty
  or one-frame-late board was observed, so no extra regression test was added beyond the live
  receipts and screenshots.

## Core-layer VMCE decoupling (B5, 2026-11-12)

Refactor-only series after the merged baseline; no protocol, gameplay, serialization or hash change.
The series is `da212a8a4`, `86ca439a6`, `e8cfdcb93`, `dff07c1d9`, plus `7a722853c`
(critic fixes) and `1a5e02431` (byte pins and guard hardening).

- **Done:** the VMCE-only LAN records (`0x36`-`0x3E` and the `VmceSignal` kinds `0x37`-`0x39`)
  moved from `src/net/lan_messages/cooperative.rs` to `src/net/lan_messages/vmce.rs`; the shared
  module keeps the native BC6 records. The LAN intermission transport moved from `src/net/udp.rs`
  to `src/net/udp/vmce.rs`; with the feature off the field, module and state are absent, and the
  shared owner keeps only clear gated access points. The VMCE campaign session helpers moved from
  `src/sim/scenario_session.rs` to `src/sim/scenario_session/vmce.rs`; the fields remain on
  `ScenarioSession` as their only owner, and the three `fold_identity` folds stay at their original
  positions with the same inputs.
- **Guard:** a production `#[cfg(feature = "vmce")]` gate-count guard records shared-owner gates
  (556 -> 503 across 48 shared files; the 193 gates in diagnostics tooling are pinned separately). The number is a change record, not evidence that decoupling is
  complete; the remaining shared access points and mode-check callers are deliberate.
- **Validation (current HEAD `1a5e02431`, not the `70d569b86` baseline):** focused core tests 159
  default / 195 vmce, app 275 default / 396 vmce (4 ignored), architecture guards 26 in both
  configurations (503 shared + 193 diagnostics pinned), production `check -p vera20k --lib --bins`
  in both configurations. Labelled build `b5-core-1a5e02431-r1` (exe SHA-256
  `DB2FD58B58655D3D0A7B1C4A84086E743BD5AC9C52E4AEC99D6F1E8CEE8E7074`) passed `2p-win` (300
  matching checksums) and `2p-retry` (300 matching checksums, no failure) LAN acceptance; earlier
  stages also passed on `b5-udp-e8cfdcb93-r1` and `b5-core-dff07c1d9-r1`. Evidence is under
  `.scratch/b5/` (not committed). Two read-only critics reviewed the series: APPROVE WITH NOTES, no
  blocking findings; the actionable notes are addressed in `7a722853c` and `1a5e02431`.
- **Residual:** `finish_lan_intermission` and `cancel_retired_lan_sends_to` still have no direct UDP
  test (pre-existing gap; behavior unchanged by the moves).
- **Still open, not closed by the file moves:** B40 (map-details cache inside the per-frame setup
  view), B41 (synthetic catalog record on intermission), B10 (terminal LAN delivery) and the
  connected-validation items above. The moves change ownership, not those behaviors.
- **Follow-up (closed in B106, below): campaign data vs lobby reverse dependency.** LAN and the
  result flow no longer reach campaign data through the lobby; see the B106 section below.

## Campaign data owner (B106, 2026-11-12, closed)

Closed by `a290f94de`. The VMCE campaign series reading, identity, whole-series digest and
next-stage resolution moved from the lobby setup module into `src/app/vmce_campaign.rs`; the lobby,
LAN admission and the result flow now call that owner directly.

- **Owner:** series root and `campaign.ini` manifest parsing/validation, host/guest map config
  resolution, `local_vmce_series_manifest`/`LocalVmceSeries`, `host_vmce_identity`,
  `series_content_sha256`, `resolve_next_vmce_stage`/`VmceResolvedStage`, `unlock_catalog_stage`
  and the catalog discovery that reads campaign files.
- **Lobby:** `refresh_vmce_entries` is the only remaining campaign seam; it removes stale VMCE
  catalog records and inserts the discovered ones, taking its inputs from `AppState`.
- **LAN and result:** `lan_service/vmce_series`, `lan_service/vmce_setup/options`,
  `vmce_result/victory`, `vmce_round` and the A03 diagnostic call `vmce_campaign` directly; no
  module reaches campaign data through the lobby.
- **Single owners kept:** the map loader/source, progress store, session/network config and wire
  protocol are unchanged; the move reuses them instead of copying parsing or state.
- **Validation (current HEAD `a290f94de`):** campaign owner 9 tests, LAN/series/setup/result/round/
  lobby consumers 66 vmce + 12 default, architecture guards 26 in both configurations (504 shared
  + 193 diagnostics pins), production `check -p vera20k --lib --bins` in both configurations, and
  the labelled build `b106-a290f94de-r1` (exe SHA-256
  `4F7D51C7AA3DFC08C46AD4BF1FD72075903EC483F6C7282C6D8454FC2A0D4B3F`) passing `2p-win` (campaign
  start, next stage, 300 matching checksums) and `2p-retry` (300 matching checksums) LAN
  acceptance. Evidence is under `.scratch/b106/` (not committed). A read-only critic reviewed the
  commit: APPROVE WITH NOTES, no blocking findings.
- **Residual:** `refresh_vmce_entries`/`discover_entries` has no direct retain/extend/early-return
  test (the old refresh was also untested); `listed_houses` stays duplicated between the owner and
  `map/vmce_config.rs` (pre-existing, different return shape, not introduced here); the A03
  diagnostic still re-parses `[Campaign]` from `campaign.ini` for its owned-map mutation fixture
  (diagnostics-only, pre-existing, not a production owner).

## Connected validation (B36)

B33–B35 have working implementations with focused tests and local multi-process coverage. The
authoritative open-defect list is the [Chinese consolidated review](zh-CN/vmce-review.md); before
delivery, fix its current P1/P2 items and complete connected validation with real two-, three-
and four-player sessions. VMCE has up to four human roles and no observers.

- Exercise allied and hostile roles, map-driven alliance changes, AI-triggered wins, simultaneous
  outcomes, elimination and respawn, mixed results, and all-loss retry.
- Cover result-page entry animations, the ready gate, intermission role reselection, Random roles,
  joins and leaves, capacity rejection, host start, final-stage completion and return to the main
  menu. Wait for the relevant animations instead of judging an intermediate frame.
- Check every peer's results, command grants, fog, next-stage identity and deterministic
  checksums, including loss, delay, disconnect, campaign-file mismatch and load failure.
- Check intermission guests: after admission and whole-series verification they unlock only the
  designated next stage, never earlier stages; returning disconnected players follow the same
  rule; a room restarted from an unlocked middle stage starts correctly.
- Whole-series mismatch coverage exists only as focused tests (background scan, Ready/Go
  revalidation, intermission unlock); a real connected capture is missing.
- Prove the simultaneous-outcome order, alliance direction and the "House did not set a native
  result flag" fallback against original evidence, or label them explicitly as VMCE rules; local
  determinism checks do not establish original behavior.
- Locally the current build passes two-, three- and four-player win → next-mission captures,
  two-player retry and a two-player mixed-outcome advance with 300 matching checksums, and each
  peer's local `VmceProgress.json` contains only the designated next stage; the standard runner
  accepts `--players 2|3|4` and a `2p-final` scenario. Fresh audit captures also include
  `4p-win --players 3`, `2p-final` and the manifest-extra probe. These are not cross-machine
  runs, and a preview-capable host lobby was captured with `XMP29U2.MAP` as a series stage.
  Guest-side intermission preview, the full-room LAN listing and long-name statistics remain
  unobserved (A44/A45/A38).
- The complete credits roll and network failures remain unverified.

## Required reliability and admission work

- **B57 — unlock timing (closed 2026-11-13, `9de82144e`):** the result agreement persists the
  next-stage unlock immediately and idempotently on every peer, before Continue, the intermission
  room or loading the next stage; a solo host uses the same path. The poll writes right after each
  received round record, so a following Left, delivery failure or phase close cannot skip it, and
  a failed store write is not remembered as saved (the next poll retries). A retry, a campaign end,
  a stale result nonce and a mismatched result or stage expose no unlock. The progress store stays
  the only writer. See the [Chinese audit](zh-CN/vmce-review.md). Validation: unit tests for the
  agreement-time decision (host, guest, solo, retry/campaign end, proposal-only, per-round nonce
  memory), and labelled build `b57-9de82144e-r1` (exe SHA-256
  `7B49F5B22AEB6AA19B999BE190991A6F5D9660DEAC95277F15B44FDDF2AB3D3C`) passing `2p-win` and
  `2p-retry` LAN acceptance; both `2p-win` peers' kept `VmceProgress.json` contain
  `stage02/all01umd.map`, and the retry run writes none. Residuals: the poll early-return ordering
  and the literal "leave the result page right after agreement" capture have no direct test; a
  same-batch proposal + confirmation + `Left` can exit before the local confirmation is sent
  (below the agreement gate, so no write); a clean `Left` shrinks the connected set, so the
  remaining host may write its own locally resolved, digest-bound stage even though the departed
  peer never confirmed (Continue still waits for every row). The trigger is unit-tested and runs
  before those exits.
- **B58 — Ready/Go revalidation (closed in B4, `6322d411b`):** Ready/Go/GuestPrepare now
  hand off a consumable revision-bound ticket and the current handoff fails closed; focused
  production regressions cover a changed unselected map, a restored map and stale tickets.
  The original same-machine acceptance (host and guest refuse Go after a map change, recover
  after restoring, duplicate and late scans) still needs a real capture.
- **B59 — distinct role countries:** launch verifies distinct Houses but not distinct Countries;
  validate country identity in the shared launch owner and report the conflicting roles. Also
  resolve the ambiguity when an AI and a player share a Country: the first House selected for
  an alliance victory can differ from the player. See [audit A04](zh-CN/vmce-review.md).
- **B61 — retry a failed series scan:** the same identity retains mismatch/failure after the guest
  fixes a later map. Offer an explicit retry or invalidate the failed result on a user action.
- **B62 — local unlock failure feedback:** corrupt or unwritable progress is logged only. Preserve
  the file, report the local save failure and offer a retry.
- **B64 — round-control authority (closed in B4):** an admitted guest can send `Begin`/`Go` to peers without
  the receiver checking that the sender is the host; Ack also lacks host and round-phase checks.
  Authenticate the sender and round identity; test premature, forged, delayed and duplicate
  messages without breaking normal retransmission.
- **B65 — result-message ordering (closed in B4):** Options delivered before Begin can be acknowledged by
  transport without producing the round ACK, leaving the host waiting. Track both facts and
  acknowledge idempotently once both arrive; test loss, reordering and duplicates.
- **B68 — single-role room capacity:** `PlayerCount=1` is valid but room limits clamp to two,
  so a second participant can join and prevent a start. Reject new excess joins while preserving
  already-present players when a campaign advances to a smaller stage.
- **B69 — control-message backpressure:** queue-full failures for Ready, votes, Begin, Options
  and ACK are ignored. Retain requests until every destination accepts them, including partial
  admission in four-player rooms.
- **B70 — shared-House building commands:** authorized shared buildings and walls cannot be
  repaired/sold through all input and network checks; apply the existing House grants end to end.
- **B71 — passive primary House results:** passive Houses are filtered from score rows, but the
  result round uses the row count as its participant count. Use frozen participants and define
  whether passive primary roles are supported before admission.
- **B72 — changed map and result page:** results re-read the finished map and may lose their VMCE
  board after a disk change. Project the frozen mission identity or fail clearly and reliably.
- **B74 — solo campaign start (closed in B4):** the ordinary minimum-opponent gate rejects a solo VMCE host
  without lobby AI. Admit the supported solo campaign through the authoritative start path.
- **B75 — shared-unit deployment and unloading:** self-click actions and some D-key unloads
  require the primary House despite a shared grant. Reuse the grant for input and retain command
  authorization. See [audit A23](zh-CN/vmce-review.md).
- **B76 — retry-load failure admission (closed in B4):** the failure signal is not tied to an admitted sender,
  VMCE loading phase or attempt and may abort an unrelated network load. Validate all three;
  exercise invalid and stale signals. See [audit A24](zh-CN/vmce-review.md).
- **B77 — load-failure delivery (closed in B4):** the reliable queue is reset immediately after enqueueing a
  local load-failure notice, racing delivery and acknowledgement. Drain it with a bounded failure
  path before cleanup. See [audit A25](zh-CN/vmce-review.md).
- **B79 — campaign AllowWin gate (closed in B4; end-to-end acceptance open):** VMCE now applies the campaign tag-count gate to each
  House's victory, while another player's ready result can end the mission first. House-level
  regression tests pass; an action-15 test map and native/multiplayer acceptance remain open.
  See [audit A27](zh-CN/vmce-review.md).
- **B81 — cross-platform map paths:** manifest validation normalizes backslashes, but loading may
  treat them literally on Linux/macOS. Normalize consistently and retain path containment checks.
  See [audit A29](zh-CN/vmce-review.md).
- **B82 — equivalent INI sections:** case/separator variants of the same logical stage or House
  control section can pass duplicate checks and silently select the first. Reject ambiguity;
  see [audit A30](zh-CN/vmce-review.md).
- **B83 — construction failure notification (closed in B4):** failed construction can consume the loading
  session before VMCE failure detection, so no notification is queued. Preserve the frozen
  attempt identity; see [audit A31](zh-CN/vmce-review.md).
- **B84 — final installation failure (closed in B4):** a failed network installation shows a local error without
  notifying peers. Use the reliable shared failure/exit path; see [audit A32](zh-CN/vmce-review.md).
- **B85 — non-ASCII map filenames:** external configuration widens bytes rather than decoding
  UTF-8, so manifest filenames may not match disk. Define VMCE configuration encoding and peer
  identities; see [audit A33](zh-CN/vmce-review.md).
- **B87 — out-of-range control role:** a `PlayerNControl` key whose role number exceeds the
  stage's `PlayerCount` is accepted and then never reaches a role, so a map-author typo silently
  leaves a shared House AI. Validate the control index against `PlayerCount` at admission and name
  the stage/House/key; see [audit A35](zh-CN/vmce-review.md).
- **B88 — stale next-stage identity records:** the VERA `0x36` stage record carries no round or
  epoch binding, and the retire-phase hold queue replays records after a later round opens. An old
  record therefore rejects a valid stage and returns everyone to the lobby, and any peer can send
  one. Bind the record to the round and accept one per peer per round; see
  [audit A36](zh-CN/vmce-review.md).
- **B92 — stale result records before the nonce check:** the result record compares per-slot
  entries and the stage digest before checking the round nonce, so a replayed record from the
  previous round (same per-slot outcomes) fails the digest and makes the receiver quit the round
  back to the main menu. Check the nonce first, drop foreign-round records and keep teardown only
  for real disagreement; see [audit A39](zh-CN/vmce-review.md).
- **B94 — round-phase liveness:** the VMCE result/intermission round sends no BC6 20 s keepalive
  (`0x0046EADD`) and the LAN pass returns before the setup keepalives and lease polling, so an
  idle page or a silently stopped peer is never detected and the wait can last forever. Send the
  round keepalive and keep the existing silent-peer check running; see
  [audit A40](zh-CN/vmce-review.md).
- **B95 — map cinematics in VMCE:** a map's `[Basic]` `Intro`/`Brief`/`Action`/`Win`/`Lose`/
  `PostScore`/`PreMapSelect` movies play on the campaign-start and BC6 continuation paths, but VMCE
  mission start, win/lose result and the next-stage handoff go straight to the result page or
  `begin_loading`, so the films are skipped. Play them through the existing fullscreen-movie owner
  on each local peer; see [audit A41](zh-CN/vmce-review.md).
- **B97 — disabled Change Map paints enabled:** in an intermission room `vmce_intermission_map` locks
  the map and `SetupAction::ChooseMap` returns early, but the setup renderer passes a hard-coded
  `false` disabled flag for the button, so it keeps the enabled red face and bright caption and
  clicks silently do nothing. The guest dialog does not show Change Map at all, so the defect is
  host-side. Drive paint from the same `enabled(CHOOSE_MAP)` test; see
  [audit A42](zh-CN/vmce-review.md).
- **B98 — manifest key validation:** the `[Campaign]` section reads only known keys and silently
  ignores unknown ones, and `ScenarioN` rows beyond `ScenarioCount` are neither read nor rejected;
  the scenario sections already reject `PlayerN` beyond `PlayerCount`. A low `ScenarioCount` or a
  stray/typoed key therefore drops a stage without an admission error. Whitelist `[Campaign]` keys
  and reject out-of-range `ScenarioN`; see [audit A43](zh-CN/vmce-review.md).
- **B99 — intermission preview and briefing hint for guests:** a new intermission setup protocol
  starts with `local_map_verified=false`; the next stage metadata is already in the session, so a
  guest's first host options compare equal and `map_changed=false`, leaving the preview flag false
  and `local_map_index()` empty. VMCE LAN never sends a received preview, so the guest's preview is
  blank and the "map has a briefing" chat hint is skipped. A newcomer's `open_lan_joined_setup`
  also skips `refresh_vmce_entries`. Seed the verified local map source into the new protocol and
  ensure the intermission stage record whenever options carry `vmce_intermission`; see
  [audit A44](zh-CN/vmce-review.md).
- **B100 — full VERA room advertised as open:** the host answers LAN Discovery with
  `advertisement(name, true)` regardless of `network_room_full()`, so a full intermission room
  (2/2 or 4/4) still looks joinable and a click ends in `TXT_GAME_FULL`. Use
  `!session.network_room_full()` (plus the editability state) for the open bit; see
  [audit A45](zh-CN/vmce-review.md).
- **B10 — terminal LAN delivery.** `release_transport` currently services once and resets the
  scoped reliable queue; losing the final Go or a rejection can strand a peer. Keep the
  existing transport and retry owner until the relevant records are acknowledged or a delivery
  failure is handled explicitly. Cover final Go, mismatch/rejection, departure and a peer that
  stops responding, with packet/ACK loss and real multi-process validation. The campaign-end
  portion is not fully validated; B10 as a whole is open.
- **B03 — setup rollback:** implemented; only the real LAN failure/notice capture remains,
  including rollback and room exit. Do not describe its unit test as a real capture.
- **B13 — guest role propagation:** implemented with focused tests; exercise a guest changing
  role in a real four-player room and confirm both other guests receive the update.
- **B05 — validate all stages:** LAN admission verifies every manifest map's bytes and the
  `campaign.ini` identity, so a peer whose later missions differ is refused before play; discovery
  still fully validates only the Entry stage's campaign configuration (unlocked stages are
  validated as they are discovered). Remaining work: validate each listed stage's configuration
  when selecting the campaign so malformed later stages are reported before play, while keeping
  the whole-series verification cost in mind.
- **B11 — long stage paths:** the LAN stage record has a 215-byte path limit and fails closed.
  Decide and implement a compatible representation or a clear admission-time limit; test the
  boundary on each supported platform.
- **B14/B16 — error reporting:** review the discovery-filtered invalid-campaign notice and
  localized-string matching; preserve structured error identity. Make setup/freeze failures
  actionable without misattributing all players to the local slot. A probe with a case-mismatched
  `Player2` House was rejected during discovery and only logged, with no lobby notice.

## Evidence and documentation maintenance

- **B07/B09/B19:** implemented entries moved from this ledger into Completed work; keep the
  guides, hub and baseline consistent with actual behavior. Current guides state that series CSF
  is not checked. Older pre-merge receipts do not validate the current build.
- **B17/B18:** standard final-stage coverage now exists as the `2p-final` scenario; still add
  setup-failure, role-change and RTL coverage, and improve phase deadlines and retail-copy reuse.
  A two-process final probe reached credits and returned after Escape with matching checksums; it
  did not watch the full credits roll. The earlier `vera20k.exe` startup error `0xc0000142` was a
  local Windows environment issue, resolved on the machine; it is not a VMCE code defect.
- **B08/B22:** English and zh-CN have VMCE text; other shipped languages currently use English
  placeholders. A translation pass is later work; native CSF follows its installed language.
- **B91 — synthetic result/defeat capture fixtures:** the `vmce-result-*` and `vmce-defeat-*`
  shell-capture checkpoints fail with `VMCE defeat fixture needs authored role metadata` in a
  default captured settings tree. The lobby fixture caches role details under
  `game_config.paths.ra2_dir` (`install_vmce_lobby_fixture`), while `vmce_map_details` looks the
  cache up under `settings_dir/Maps`; the miss then depends on a second map load that does not
  resolve in a score capture. Workaround verified: an authored-role map at
  `settings/Maps/<catalog map>` (in the audit run, `settings/Maps/XMP29U2.MAP`) makes the fallback
  resolve and all ten result/defeat checkpoints render. The cache root/file identity mismatch
  remains the fix target; the fixtures also need configurable player names before they can close
  [A38](zh-CN/vmce-review.md), and the shots can still close the visual part of
  [A37](zh-CN/vmce-review.md).
- **B93 — acceptance runner retail marker:** `vmce_lan_acceptance run` writes the
  `.vera20k-lan-capture-root` marker into each per-process settings directory, but the capture
  path checks the marker under `assets.ra2_dir()`, so a plain `--retail-source` fails with
  `capture writes require an explicitly marked isolated directory`. Write the marker into each
  retail copy (or require and validate a marked source) and document the requirement. The real
  `2p-retry` capture used a previously marked retail copy as a workaround.

## Not implemented / follow-up work

These are implementation gaps and follow-up work, not deferred audit items.

- **B20/B21:** HUD and loading/transition presentation, including useful wait/failure status.
- **B23:** `AltNextScenario`, `SkipMapSelect` and cross-mission money are not implemented or
  included in B33–B36. Intermission role reselection, whole-series admission and local unlocks
  are implemented and covered by B36 acceptance. In-mission save/load, reconnect, host migration
  and AI takeover remain open.
- **B02:** result-page timers and forced pulls were cancelled; do not introduce them.

## Lobby UI review residuals

- **B60 — oversubscribed intermission:** extra participant rows can overlap the briefing because
  the layout reserves only the next map's roles; start refusal is also a generic English notice.
  Keep the room, give every player a usable row and show the localized player/role limits.
- **B63 — diplomacy mode name:** the Chinese diplomacy screen renders `Multiplayer Campaign`
  literally instead of using the existing localized VMCE title.
- **B66 — briefing lookup:** VMCE's directory-qualified map name cannot match retail
  `MISSIONMD.INI` sections keyed by filename; use the filename for briefing lookup and capture it.
- **B67 — solo hint:** guests and multiplayer hosts also see the single-host Start hint. Show it
  only to a lone host and provide appropriate Ready/waiting guidance to other participants.
- **B73 — stale lobby AI:** AI slots from an ordinary LAN room can survive switching into VMCE
  and occupy seats without matching campaign roles. Clear lobby AI while preserving map AI.
- **B78 — next-stage failure feedback:** if resolving the next stage fails after victory, players
  can leave without seeing results or a reason. Preserve the outcome, explain the error and exit
  reliably. See [audit A26](zh-CN/vmce-review.md).
- **B80 — in-mission Observe (closed in B4):** the abort dialog may offer Observe and queue local destruction.
  Exclude that action in VMCE while preserving ordinary LAN; see [audit A28](zh-CN/vmce-review.md).
- **B86 — lobby shared-House visibility:** the role board lists the primary role House and country
  only, so the shared Houses granted through `PlayerNControl` are invisible before the mission.
  Project each role's granted Houses from the parsed `campaign.ini` control owner; keep the 2–4
  role layouts and scales clear. See [audit A34](zh-CN/vmce-review.md).
- **B89 — defeat-page retry vote feedback:** the BC6 failure page shows only the local Retry light
  and the host's all-voted aggregate, so 3–4 player rooms cannot see who has voted. Render per-row
  vote state from the existing `retry_votes` data; see [audit A37](zh-CN/vmce-review.md). A real
  two-process `2p-retry` capture confirmed the page has no per-row state; 3–4 player capture
  remains.
- **B90 — statistics-page name truncation:** the victory/campaign-end statistics page draws the
  player name into a fixed cell without the native ellipsization the result board and failure page
  already use, so long names run into the kills/losses columns. Reuse
  `vmce_panel::ellipsize_native`; see [audit A38](zh-CN/vmce-review.md).

The lobby layout and chrome pass (see Completed work) fixed most review items; these remain open.

- **B40 — map-details cache dependency:** `vmce_map_details` runs inside the per-frame setup view
  and parses the map INI on a cache miss; the cache exists only while `network_setup_protocol` is
  present, so protocol-less diagnostic frames can re-parse it every frame. Move the cache to an
  always-present owner or add a frame-time guard.
- **B41 — synthetic catalog record:** `open_vmce_intermission` pushes a scenario record when the
  next stage is not found; the lookup ignores the series directory and the push bypasses the
  catalog owner, so a repeated open could duplicate the record. Deduplicate by
  `(series directory, map file)` or move the ensure-stage responsibility into the catalog owner.
- **B43 — Random roles invisible:** newly admitted participants default to Random, but the board
  maps only concrete roles 0..3, so Random players have no row and no Ready glyph; a room can show
  blank or partial rows while its players are present. Show a localized Random placeholder row per
  Random participant, or an inline "N players use Random" line, without changing assignment timing.
- **B45 — briefing truncation:** the lobby briefing is drawn into a box that shrinks from 118 to
  64 px when the status strip shows, with no ellipsis or scroll, so long `MISSIONMD.INI` briefings
  lose their tail silently. Mark the cut with "..." as the enlarged preview does, or stop shrinking
  the box.
- **B46 — verification feedback:** a pending whole-series scan maps to no inline status and Ready is
  gated by `accept_eligible`, so a guest sees an inert Ready button for the scan's duration. Show a
  checking-campaign status while Pending and distinguish failed or cancelled.
- **B51 — display-only affordance:** the board uses the native list frame, yellow values and the same
  text size as the editable combos but has no hover, focus or selection state, so it reads as an
  interactive list. Use a dimmer value tone or a small caption to mark it read-only, or make a cell
  genuinely interactive.
- **B52 — briefing framing:** the mission briefing is the only full-width block without a panel
  (`x=74, w=558`, plain text), giving lines of about 90 characters under two framed panels. Frame it
  like the board or constrain it to the board column with a reading measure of about 450–500 px.
- **B54 — layout stability:** showing or hiding the status strip switches the briefing box between
  118 and 64 px and moves the text below it, including when a background series scan completes.
  Reserve the strip area always, or merge the status into the board footer, so the reading position
  does not jump.

## Implementation and evidence limits

- **B33 — per-player outcomes and in-mission participation:** the main path is implemented; the
  distinct-country admission rule still needs B59. Locked decisions: a
  country-index victory makes the triggering House's allies winners and non-allied players losers,
  including an AI trigger; only map triggers change alliances; the first player to reach the
  native result boundary ends the mission for everyone and the other results are determined from
  map/House state at that boundary; a House that set no native result flag uses the latest map
  outcome action's decision (a VMCE rule); an eliminated primary House does not end the mission,
  grant whole-map vision, loss EVA or an input lock, and units later spawned for it stay
  controllable; map-authored `PlayerNControl` grants do not change with diplomacy; role countries
  stay distinct; any player winner lets everyone continue; with no player winner, guests vote and
  the host retries the same frozen roles, map and seed. Evidence: local multi-process 2-, 3- and
  4-player win/advance, 2-player mixed-outcome advance, trigger-driven alliance break, all-loss
  retry and final credits-skip runs, repeated after the result-digest and whole-series admission
  changes. Connected and original-evidence gaps are in B36.
- **B34 — intermission room and role selection:** implemented. All connected guests ready on the
  result page and the host continues; everyone returns to the campaign room with the designated
  next stage, reselects a unique role or Random (Random avoids taken roles) and readies; joins and
  leaves are allowed between stages; if the next stage has fewer roles than players the room stays
  open and Start is blocked; the host cannot replace the designated stage; a retry does not
  reselect roles; the last stage enters completion instead of a room. Evidence: intermission
  captures after 2-, 3- and 4-player advances. New-guest and capacity scenarios are in B36.
- **B35 — persistent local unlocks and whole-series admission:** the main path is implemented;
  B57, B61 and B62 remain open (B58 closed in B4). Locked decisions:
  unlocking does not require loading the next stage, and it is written only after every online
  participant confirms the round's result and next-stage identity — the result record binds the
  round nonce to a digest of the resolved stage, so opening the page writes nothing and a peer
  whose result or stage differed never confirms; records without that digest are refused. A host
  starting a new room may choose a locally unlocked stage; guests do not need prior unlocks but
  must pass the same campaign-file verification. An intermission guest unlocks only the
  designated next stage, never earlier stages, including after a reconnect; unlocks survive
  content updates under the same campaign directory and stage path, while renaming either creates
  a new identity. LAN admission verifies the whole series (`campaign.ini` bytes, manifest order,
  every manifest map's relative path, length and bytes): the host advertises one digest, a guest
  verifies it in a cancellable background worker, and a pending or failed scan keeps the guest
  unready and Start closed; Ready/Go may reuse a prior revalidation (B58), while the selected map and
  `campaign.ini` remain individually checked, and the series CSF is not part of the digest.
  Branches, if added later, unlock only the branch actually selected. Evidence: after the 2-, 3-
  and 4-player captures every peer's `VmceProgress.json` contained only the designated next
  stage; whole-series mismatch has focused tests only, connected coverage is in B36.
- **B15 — host manifest race:** VMCE LAN has a whole-series verification path; B58 (closed in
  B4) replaced the reused scan with a revision-bound revalidation. The freeze boundary still re-reads
  `campaign.ini` and the selected map. A series change after revalidation can remain undetected.
- **B01:** same-stage two-player retry and vote were exercised over LAN with 300 matching
  checksums; four-player retry and the real load-failure notice remain unverified.
- **B12:** VMCE lobby admission rejects observers; an existing observer must become a player or
  leave before switching into VMCE. The in-mission Observe action still has the B80 defect.
- **B30–B32:** defeat statistics use frozen player/House assignments; victory result boards
  use the finished map and frozen assignments, and Ready state refreshes. Real 2p/3p/4p victory
  captures waited for the entry animation and used the page controls. A reversed source/seat
  2p retry checked statistics and colours. A two-player mixed-outcome advance passes locally, so
  per-player outcome boards are exercised; connected mixed-outcome validation is still open (B36).
- **B24–B28:** lobby/result UI work was implemented; four-row BC6 is a VMCE extension. Income
  is gross positive credits earned and is display-only. **B29** is a local-only UI skill branch;
  its retention does not establish runtime correctness.
- Campaign completion was fixed to leave the LAN visit before returning to the main menu.
  The final Go reliability issue B10 remains separate.
- **Lobby layout and chrome pass (B37–B39, B42, B44, B47–B50, B53, B55, B56):** implemented.
  The owner combo is 120 px wide (a 100 px text face) and the board reflows to 298..632 with country
  text 150, Ready 48 and the colour column on its native anchor; labels and captions share y 14.
  The 96/120 px combo cells paint VERA's native-toned field (shared fill plus the native list-frame
  bevel), because no native combo face covers those widths. One palette replaces the four unrelated
  blacks and the gold hairline: board/field fill, muted empty swatch, list-frame bevel and a warning
  colour for the inline status, which now spans the room width and ellipsizes its second line.
  Unclaimed roles show the native waiting label; AI rows no longer show an owner combo; board
  country and waiting text are measured in authored pixels, and every cut marker is "..."; the dead
  `layout.flags`/`VmceLobbyRole.role` code and the literal fill height are gone. Evidence: focused
  tests (`ui::coop_lobby` 5, `skirmish_shell_render::vmce_panel` 3, `app::shell_network_setup` 32,
  `skirmish_shell_render` 100, `ui::multiplayer_setup` 10) and a default-feature build; no capture
  or screenshot, so palette contrast and the combo field seam remain visually unverified.
- **Map-driven lobby slots:** the role board, player rows and the host's room limit follow the
  selected map's authored primary roles (2–4); the board height, status strip and briefing move
  with them, so a three-role map shows three seats. Combo bevels stay inside their cells and the
  caption rule draws in device pixels at every shell scale. Evidence: `ui::coop_lobby` 6,
  `skirmish_shell_render::vmce_panel` 4, `app::shell_network_setup` 32,
  `app::diagnostics::shell_capture` 94, `skirmish_shell_render` 102, `campaign_score_shell` 16 and
  both feature checks; no capture, so the scaled-shell look is still unverified.

