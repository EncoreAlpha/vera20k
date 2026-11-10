# VMCE backlog

Living task ledger for known VMCE gaps. Severity: S1 blocks the next release, S2 is a
visible correctness/claim gap, S3 is polish/robustness. Evidence for each item is in
`.scratch/vmce-logic-audit/` (local audit reports, not committed) and the cited source.

> Current work order: B03, then the remaining S1/S2 items. B01 is accepted: genuine VMCE 2p-retry receipt (same VMCE stage, 300 equal checksums) and the live BC6 table (rows matching admitted players, lobby names, bare times, localized headers) pass on host and guest. The B24–B28 UI batch is complete; B29 (VMCE UI skill) is on the local-only branch `feature/vmce-ui-skill` at `ad7f27e2e` and is never merged or pushed.

## UI — completed batch (B24–B28)

All five items were implemented, capture-reviewed (`.scratch/vmce-ui-batch/b24`…`b28`) and committed. They are
recorded here for residuals only.

- **B24 Lobby/result column revision — completed.** Lobby board is
  `玩家 | 所属方 | 国家 | 准备 | 颜色`; the role/Role display column and the separate 主机 subcolumn are gone;
  the Owner column shows only the map House name. Result rows carry the lobby player name (B25 follow-up) and the
  native H/red-star Ready glyphs. 800/1920 captures recorded.
- **B25 Result-page colour/Ready/Leave revision — completed.** Victory/campaign-end `0x103` rows are
  `玩家 | 所属方 | 国家 | 结果 | 准备`, all row text in the role's House colour with no row backdrop
  strips; native yellow headers/icons. The board fill ends at the final Ready cell (no blank strip below
  the last row). Right panel: row one is host `继续` / guest `准备` (Continue frame 2 disabled until the
  all-ready gate opens, then frame 4 lit; Ready frame 2 until clicked, then lit and non-clickable), row
  two is `统计数据`, bottom slot is `离开房间` for both roles. Captions are centred in lit and disabled
  states. The board sits clear of the native time panels, status line and right-panel buttons.

- **B26 Defeat page on the native BC6 dialog — completed.** Defeat uses `0xBC6` with four
  role rows and the same six-column statistics table as the victory stats page (total/par time
  header, `玩家 | 击毁 | 损失 | 建造 | 分数 | 收入`, four rows in House colours, no row strips; BC6's
  own five-column table/headers hidden). BC6 art, the four-row extension and eagle overlap remain.
  Continue is Retry: a guest click casts one `0x39` retry-vote record and lights the native frame-4
  button; the host starts disabled and lights only after every online guest voted, then runs the
  existing retry step with no timeout and no host vote. BC6 Quit is Leave Room (`0x24`; host disbands,
  guest exits locally). Native disabled/lit button art reused, no new bitmap. **Residual:** BC6
  natively lays out two player rows; VMCE's four-row page is a non-native extension. Real-LAN
  retry-vote validation is deferred until B01 restores retry loading.

- **B27 Victory-page `0x103` statistics page — completed.** Victory/campaign-end hide the top
  report's `摧毁敌军数` label and value (native panel height kept). The right panel is row-one
  Continue/Ready, row-two `统计数据`, bottom Leave Room, for host and guest; there is no left tab.
  The full-screen stats page has only a normal (never lit) `返回` in the bottom right-panel slot; it
  uses the BC6-style artwork backdrop (the 0x103 SCRT armoured-eagle panel is skipped), the header
  shows total/par time, and the table has four rows and six columns
  (`玩家 | 击毁 | 损失 | 建造 | 分数 | 收入`) in House colours with no black row strips. Defeat (BC6)
  shows the same table and has no separate stats entry. **Income residual:** `credits_earned` is a
  display-only accumulator in the existing sim `Economy` owner, fed from positive `Add_Credits` and
  `Add_Tiberium_Credits` wallet gains (gross earned); it excludes the starting balance, spending/losses
  and campaign carry-over. It is serde-defaulted for old saves and hash-retained with a zero stream,
  and never steers gameplay. New i18n keys: `统计数据`/`Statistics`, `收入`/`Income`, `返回`/`Back`.

## UI review follow-up (completed)

- **B28 Review feedback superseding parts of B25/B26/B27 — completed (`147c753ad`, follow-up
  `fc6da806a`).** First-slot Continue/Ready moved into the topmost native action slot
  `(644,199)`, `统计数据` on the next row `(644,241)`, Leave Room bottom `(644,535)`; every result
  row backdrop strip removed and the board fill ends at the final row; lit/disabled captions centred.
  The failure BC6 page uses the shared six-column statistics table. The stats page has no left tab,
  only a bottom normal `返回` `(644,535)`, drops all row strips, and draws on the BC6-style backdrop.
  All 800/1920 captures and zooms regenerated under `.scratch/vmce-ui-batch/b28`.
- **B29 VMCE UI conventions skill — completed locally, never merged/pushed.** Branch
  `feature/vmce-ui-skill` at `ad7f27e2e` holds `.agents/skills/vmce-ui/SKILL.md` plus the
  `.claude/skills/vmce-ui` mirror. It records native-asset policy, layout owners/rects, button rows,
  Ready/retry-vote projections, House colours, i18n policy, capture/contact-sheet workflow, focused
  tests and residuals. `tools/skill_sync.py --check` is clean. Keep this branch local only.

## Locked decisions (after the B24–B28 UI batch and B01)

- Version after the B01 fix: `0.2.9-vmce.6`. Push `feature/vmce` only after the screenshot review
  manifest is accepted; no formal Release.
- Fix order: **B03 → B13 → B12**. Release gate: VMCE `2p-retry` now passes with a genuine
  capture (BC6 page, guest votes, host retries, same stage, 300 equal checksums); `2p-win` and
  `4p-win` remain. Three-player, disconnect and RTL checks are not blocking.
- B01 is accepted: genuine VMCE 2p-retry receipt and identical host/guest live-table stats pass.

- B03: a failed setup apply aborts clearly with rollback/error reporting; no automatic retry.
- B04: closed by B26 — the BC6 defeat page no longer paints the old `0x103` ready gate.
- B05: validate every `campaign.ini` stage at host map pick, not only `Entry`.
- B06: wiping one role House marks only that role (no local defeat/EVA/full-map reveal/UI lock);
  the mission continues.
- B07: correct overstated docs only after the behaviour fixes and real captures land.
- B08: keep the English placeholders in the eight non-en/zh-CN languages; translate later.
- B10: keep/retry the transport until the key record is received before releasing it.
- B12: disable observer seats in VMCE (reject the observer role) instead of fixing seat restore.
- B13: propagate guest role changes to the 3rd/4th guests.
- B20/B21 (HUD/loading) follow the B01/B03 logic fixes.
- B22: VERA labels default to the Windows system language when unpicked; the native CSF's install
  language mismatch is documented and expected.
- B23: do none of `AltNextScenario`, `SkipMapSelect` or cross-mission money in this batch.

## S1 — release blockers

- **B01 Retry never restarts — completed (`c1dd00f83`, `21f71ea58`, `8f03b60ca`, `fac77ce6d`, `1adbc7863`, `60faac2de`).**
  The retry freeze rebuilt a `PreparedNetworkStartup` without the admitted map config/source/digest,
  so the loader aborted and both peers fell back to the lobby. `freeze_cooperative_round` now
  captures the previous frozen startup's identity before mutating network state and rebuilds with
  the same `vmce_map_config`/`local_map_source`/effective digest, seed, difficulty,
  roles/countries/colours and House assignments; no next-stage resolution, re-randomisation or
  `campaign.ini` re-read. Any retry load failure on either peer now sends a VERA-only `0x3A`
  reason (missing map, config mismatch, identity mismatch, load failure), shows one native
  blocking box in the current language on both peers, and returns both to the LAN lobby. The B26
  all-online-guest vote gate is unchanged (no timeout, host never votes, a leaving guest's vote
  clears, duplicate clicks are safe). The live BC6 statistics table now matches admitted players
  only (2p=2, 4p=4), uses lobby names, bare `HH:MM:SS` times, localized headers including `收入`,
  and projects scores by canonical network source/House so host and guest show identical
  per-player stats (host `LiveHost 16/26/90/7600`, guest `0/0/88/0` in the recorded run).
  Genuine VMCE `2p-retry` capture passes (`.scratch/vmce-retry`, VMCE checkpoints): both peers
  reach the BC6 page, the host gates on the guest vote, begin/options and ack complete, Go starts
  the retry, both load the same `VMCE/LDA-Lose/stage01/all01umd.map`, and both receipts carry the
  retried-stage evidence and 300 identical native checksums. Residuals: no 4p-retry real capture;
  the two-peer load-failure modal/lobby path has unit coverage only.


## S2 — correctness / claims

- **B04 — closed by B26.** The defeat page is now the native BC6 statistics dialog (no Ready
  column and no `0x103` ready gate); the remaining `0x103` ready gate is the real victory/next
  and all-ready gate. Correct the stale `0x89E05D` comment when that path is next touched.
- **B05 Strict campaign.ini validation covers only the Entry stage.** A malformed later stage
  plays and fails after the previous win, silently returning both peers to the lobby. Decision:
  validate every stage at host map pick before the room can start.
- **B06 Role auto-defeat side effects contradict docs.** Mid-mission role House defeat still
  fires EVA/full-map reveal/UI gating while the mission continues. Decision: wiping one role
  House marks only that role; no local defeat, EVA, full-map reveal or UI lock, and the mission
  continues.
- **B07 Docs/evidence overstate validation.** Final/retry/mismatch captures not recorded; some
  docs say implemented and unsupported in the same file. Correct both guides only after the
  behaviour fixes and real captures land.
- **B08 i18n VMCE labels are English copies.** Decision: en has real text, zh-CN keeps fork
  translations, and the other eight languages keep English placeholders; a translation pass is
  later work. Completeness tests keep checking presence only.
- **B09 "Guest cannot Ready without series CSF" is false.** Identity hashes campaign.ini + map
  bytes only; fix claim or add the missing check.
- **B10 One-shot campaign-end/rejection records can be lost** after `release_transport`; dead
  peer during the round is not detected. Decision: keep/retry the transport until the key record
  is received before releasing it.
- **B11 Next-stage identity caps catalog map path at 215 bytes** in the fixed LAN envelope;
  longer valid paths fail closed to the lobby.
- **B12 Observer seats reset to Random on map/mode change;** VMCE start rejects "observer has a
  player role" and the UI cannot restore Observer, so the room can be stuck. Decision: disable
  observer seats in VMCE and reject the observer role, instead of fixing seat restore.
- **B13 Guest role changes are not republished to 3rd/4th guests** on LAN (WOL path does).
  Decision: fix the LAN republish so 3rd/4th guests receive the change.
- **B14 Invalid-campaign modal is near-unreachable** (discovery filters the series first) and
  keyed on localized string equality; lobby status classifier matches English substrings.
- **B15 Host does not re-read its own campaign.ini** between start publish and frozen commit
  (fails closed only at next stage).

## S3 — robustness / tests

- **B16 Setup apply failures are invisible** at default log level; freeze error misattributes all
  humans to "local slot".
- **B17 Missing tests/captures:** retry-freeze/load, defeat projection,
  campaign-end; 2p-final real captures; 3-player path; RTL visual.
- **B18 Test workflow:** per-phase deadlines, short dev probe defaults, retail directory reuse.
  (Current: `--suite dev`, 120 s cap, fail-fast; phase deadlines and cache reuse still open.)
- **B19 Evidence baseline:** upstream-sync changed gameplay; old 2p/4p receipts are invalid.
  Keep new baseline in docs/vmce-baseline.md and do not compare against old runs.

## Deferred UI (not in the next batch)

- **B20 Game HUD (after B01/B03):** waiting/sync status, campaign identity (stage/seed/series),
  disconnect/failure notices; pause semantics stay native "menu does not freeze".
- **B21 Loading/transition (after B01/B03):** campaign identity, stage/seed, 4-player readiness.
- **B22 Language policy:** VERA labels default to the Windows system language when the player has
  not picked one; native CSF still follows the install language and that mismatch is expected
  and documented. The server-language filter is not rewritten this batch.
- **B23 Remaining campaign keys:** `AltNextScenario`, `SkipMapSelect`, cross-mission money/role
  carry-over — not done in this batch by decision. (B02 is cancelled: no timers by design.)

## Completed, keep out of the backlog


- Four-player roster/setup relay fix and real 4p-win acceptance.
- EndOfGame/defeat production skeleton, strict Entry-stage validation, invalid-campaign modal.
- Pause: native "menu does not freeze" verified and documented.
- VMCE lobby/result UI batch and upstream 0.2.9 sync.
- Release gate after this batch: a real VMCE `2p-retry` (vote→reopen) capture is still
  open; `2p-win` and `4p-win` remain. Version `0.2.9-vmce.6`. No formal Release; push only after review.
- B24–B27 UI batch: lobby/result columns, settlement board/Ready/Leave, BC6 defeat retry-vote page, and the
  victory/campaign-end `0x103` statistics tab/page. Residuals: BC6 four-row extension is non-native;
  `credits_earned` is gross positive income only (display-only, serde-defaulted, zero-preserving hash).
- B28 UI review follow-up: first-slot action button, bottom Leave/Back, no row strips, centred
  captions, BC6 six-column defeat table, BC6-style stats backdrop.
- B29 VMCE UI skill on the local-only `feature/vmce-ui-skill` branch (`ad7f27e2e`); never merged or pushed.
