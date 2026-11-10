# VMCE backlog

This is the English engineering ledger; the [Chinese backlog](zh-CN/vmce-backlog.md) uses the
same numbering. It lists only **unfinished** work, locked decisions and acceptance boundaries.
Implemented work moved to [Completed work and limits](#completed-work-and-limits) with its
evidence boundary; the [player and map-author guide](vmce-guide.md) describes current behavior.
**Planned** items are not implemented promises.

## Connected validation (B36)

B33–B35 are implemented in this working tree with focused tests and local multi-process coverage.
Before delivering them as one player-visible feature, complete connected validation with real
two-, three- and four-player sessions. VMCE has up to four human roles and no observers.

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
  accepts `--players 2|3|4` and a `2p-final` scenario. These are not cross-machine runs.
- The complete credits roll and network failures remain unverified.

## Required reliability and admission work

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
  actionable without misattributing all players to the local slot.

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

## Deferred work

- **B20/B21:** HUD and loading/transition presentation, including useful wait/failure status.
- **B23:** `AltNextScenario`, `SkipMapSelect` and cross-mission money are not implemented or
  included in B33–B36. Intermission role reselection, whole-series admission and local unlocks
  are implemented and covered by B36 acceptance. In-mission save/load, reconnect, host migration
  and AI takeover remain open.
- **B02:** result-page timers and forced pulls were cancelled; do not introduce them.

## Completed work and limits

- **B33 — per-player outcomes and in-mission participation:** implemented. Locked decisions: a
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
- **B35 — persistent local unlocks and whole-series admission:** implemented. Locked decisions:
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
  unready and Start closed; the Ready/Go boundary revalidates, the selected map and
  `campaign.ini` remain individually checked, and the series CSF is not part of the digest.
  Branches, if added later, unlock only the branch actually selected. Evidence: after the 2-, 3-
  and 4-player captures every peer's `VmceProgress.json` contained only the designated next
  stage; whole-series mismatch has focused tests only, connected coverage is in B36.
- **B15 — host manifest race:** implemented for VMCE LAN. The host re-verifies its whole-series
  digest against the value it advertised on the Ready/Go path before `GO` is sent, and the freeze
  boundary still re-reads `campaign.ini` and the selected map. Residual: a series change between
  that revalidation and the queued `GO` is not caught.
- **B01:** same-stage two-player retry and vote were exercised over LAN with 300 matching
  checksums; four-player retry and the real load-failure notice remain unverified.
- **B12:** VMCE rejects observers; an existing observer must become a player or leave before
  switching into VMCE. This decision remains in force.
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
