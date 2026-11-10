# VMCE backlog

This is the English engineering ledger. The [Chinese backlog](zh-CN/vmce-backlog.md)
contains the same decisions and work order. The [player and map-author guide](vmce-guide.md)
describes the current implementation; **planned** items below are not implemented promises.

## Agreed next feature: outcomes, intermission and unlocks

Deliver B33–B36 as one player-visible feature, including their necessary dependencies and
validation. Research and implementation may proceed in stages, but do not present a partial
chain as the finished feature. VMCE has up to four human roles and no observers.

### B33 — per-player outcomes and in-mission participation

- A country-index victory makes the triggering House's allies winners and non-allied players
  losers, including when the triggering House is AI. Classify against the mission's actual
  diplomacy. Only map triggers change alliances; players do not change diplomacy manually.
- Investigate original instructions and active callers for timing, conflicting simultaneous
  outcomes, alliance direction, and country-index defeat actions. Preserve deterministic shared
  results. Do not claim original behavior for details that remain unproven.
- An eliminated primary House does not itself end the mission or grant whole-map vision, loss
  EVA, or an input lock. The player retains their existing authorized shared control. Units
  later spawned for that primary House become controllable. Even if all primary Houses are
  eliminated, wait for map outcome actions. Research how an alliance change affects shared
  control before choosing that behavior. Keep role countries distinct.
- Mixed winners and losers share one result/continuation flow; show each player's win/loss,
  without a separate elimination marker. A previously eliminated player who is allied to the
  winner is shown as a winner. Any player winner allows everyone to continue;
  no player winners leads to the shared retry: guests vote and the host starts after all votes.
  A retry keeps the previous frozen roles,
  map and seed. On the last stage, mixed outcomes share the completion presentation and return
  to the main menu. Wait for the relevant result and completion animations in captures.

**Current difference:** actions 1/2 currently ignore the country argument and apply a team
outcome; actions 67/68 also produce team outcomes. Per-House defeat can still cause local loss
effects (B06). The current guide documents these behaviors until this feature ships.

### B34 — intermission room and role selection

After all connected guests ready on the result page and the host continues, return everyone
to the campaign room with the designated next stage. Everyone may reselect a unique role or
choose Random, then ready; the host starts after admission and identity checks. A guest may
join or leave between stages. If the next stage has fewer roles than players, keep the room,
show the reason and block Start until the player count fits; never create observer seats.
The host cannot replace the designated next stage in this intermission. A retry does not
reselect roles. End-of-campaign does not enter another stage room.

### B35 — persistent local unlocks

When a stage completes with a player winner and its next stage is resolved, unlock the next
stage on every participant's local device without requiring it to load. A host starting a new
room may choose a locally unlocked stage; guests need matching campaign files, not their own
prior unlock. A guest joining an intermission receives the stages this room actually visited
and its designated next stage after admission and campaign-file validation. If a room starts
from an unlocked middle stage, the guest does not receive all the host's historical unlocks:
only stages actually started or visited in this room. Keep unlocks across updates to the same
series. Stable series identity, durable storage, synchronization, validation and migration
are design work, not implemented facts. If campaign branches are implemented later, unlock
only the branch actually selected.

### B36 — connected validation and documentation

Exercise the complete chain with real 2-, 3- and 4-player sessions: allied and hostile roles,
map-driven alliance changes, AI-triggered wins, simultaneous outcomes, elimination and respawn,
mixed results, all-loss retry, result-page entry animations, next-stage role reselection,
joins/leaves, capacity rejection, final-stage credits and locally persisted unlocks. Check
all peers' results, commands, stage identities and deterministic checksums, including loss,
delay and disconnect cases. Test a guest joining an in-progress series and a room restarted
from an unlocked middle stage. Update both guides only when behavior changes, preserving the
distinction between implemented and planned. The standard acceptance runner needs a real
`2p-final` scenario; its current checker only understands win-next-stage and retry.

## Required reliability and admission work

- **B10 — terminal LAN delivery.** `release_transport` currently services once and resets the
  scoped reliable queue; losing the final Go or a rejection can strand a peer. Keep the
  existing transport and retry owner until the relevant records are acknowledged or a delivery
  failure is handled explicitly. Cover final Go, mismatch/rejection, departure and a peer that
  stops responding, with packet/ACK loss and real multi-process validation. The campaign-end
  portion is not fully validated; B10 as a whole is open.
- **B03 — setup rollback:** implemented; capture the actual LAN failure/notice path, including
  rollback and room exit. Do not describe its unit test as a real capture.
- **B13 — guest role propagation:** implemented with focused tests; exercise a guest changing
  role in a real four-player room and confirm both other guests receive the update.
- **B05 — validate all stages:** discovery checks listed files exist, but fully validates only
  Entry. Validate each listed stage's configuration and applicable map data when selecting the
  campaign; malformed later stages must be reported before play.
- **B15 — host manifest race:** revalidate the host's accepted `campaign.ini` identity before
  frozen start; do not rely solely on the later transition check. Coordinate with B05/B34/B35.
- **B11 — long stage paths:** the LAN stage record has a 215-byte path limit and fails closed.
  Decide and implement a compatible representation or a clear admission-time limit; test the
  boundary on each supported platform.
- **B14/B16 — error reporting:** review the discovery-filtered invalid-campaign notice and
  localized-string matching; preserve structured error identity. Make setup/freeze failures
  actionable without misattributing all players to the local slot.

## Evidence and documentation maintenance

- **B07/B09/B19:** remove stale claims rather than treating them as new defects. Current guides
  state that series CSF is not checked; 2p retry and 2p/4p wins have real captures. Older
  pre-merge captures do not validate the current build. Keep the baseline and both guides
  consistent with the actual runner and the scope of captures.
- **B17/B18:** add standard final-stage, three-player, failure/role-change and RTL coverage;
  improve phase deadlines and retail-copy reuse. A two-process final probe reached credits
  and returned after Escape with matching checksums; it did not watch the full credits roll.
- **B08/B22:** English and zh-CN have VMCE text; other shipped languages currently use English
  placeholders. A translation pass is later work; native CSF follows its installed language.

## Deferred work

- **B20/B21:** HUD and loading/transition presentation, including useful wait/failure status.
- **B23:** `AltNextScenario`, `SkipMapSelect` and cross-mission money are not implemented or
  included in B33–B36. Role reselection and unlocks are now in scope as B34/B35; they are not
  implemented yet. In-mission save/load, reconnect, host migration and AI takeover remain open.
- **B02:** result-page timers and forced pulls were cancelled; do not introduce them.

## Completed work and limits

- **B01:** same-stage two-player retry and vote were exercised over LAN with 300 matching
  checksums; four-player retry and the real load-failure notice remain unverified.
- **B12:** VMCE rejects observers; an existing observer must become a player or leave before
  switching into VMCE. This decision remains in force.
- **B30–B32:** defeat statistics use frozen player/House assignments; victory result boards
  use the finished map and frozen assignments, and Ready state refreshes. Real 2p/4p victory
  captures waited for the entry animation and used the page controls. A reversed source/seat
  2p retry checked statistics and colours. These checks do not prove B33's planned mixed results.
- **B24–B28:** lobby/result UI work was implemented; four-row BC6 is a VMCE extension. Income
  is gross positive credits earned and is display-only. **B29** is a local-only UI skill branch;
  its retention does not establish runtime correctness.
- Campaign completion was fixed to leave the LAN visit before returning to the main menu.
  The final Go reliability issue B10 remains separate.
