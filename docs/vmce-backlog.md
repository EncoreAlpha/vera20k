# VMCE backlog

This is the English engineering ledger; the [Chinese consolidated review](zh-CN/vmce-review.md) uses the
same numbering. It lists only **unfinished** work, locked decisions and acceptance boundaries.
Implemented paths appear under [Implementation and evidence limits](#implementation-and-evidence-limits);
an implemented path can still have an open defect. The [player and map-author guide](vmce-guide.md) describes current behavior.
**Planned** items are not implemented promises.

## Connected validation (B36)

B33–B35 have working implementations with focused tests and local multi-process coverage, but
review found the open defects B57–B59 and B61–B62. Before delivery, fix those defects and complete connected validation with real
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

- **B57 — unlock timing:** agreement alone does not persist the next-stage unlock; leaving or
  waiting on the result page can lose it. Persist idempotently when the round's result and stage
  identity are agreed. See the [Chinese audit](zh-CN/vmce-review.md).
- **B58 — Ready/Go revalidation:** a single series rescan is reused across distinct boundaries;
  changing an unselected map after Ready can pass Go. Use a consumable revision-bound scan result.
- **B59 — distinct role countries:** launch verifies distinct Houses but not distinct Countries;
  validate country identity in the shared launch owner and report the conflicting roles. Also
  resolve the ambiguity when an AI and a player share a Country: the first House selected for
  an alliance victory can differ from the player. See [audit A04](zh-CN/vmce-review.md).
- **B61 — retry a failed series scan:** the same identity retains mismatch/failure after the guest
  fixes a later map. Offer an explicit retry or invalidate the failed result on a user action.
- **B62 — local unlock failure feedback:** corrupt or unwritable progress is logged only. Preserve
  the file, report the local save failure and offer a retry.
- **B64 — round-control authority:** an admitted guest can send `Begin`/`Go` to peers without
  the receiver checking that the sender is the host; Ack also lacks host and round-phase checks.
  Authenticate the sender and round identity; test premature, forged, delayed and duplicate
  messages without breaking normal retransmission.
- **B65 — result-message ordering:** Options delivered before Begin can be acknowledged by
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
- **B74 — solo campaign start:** the ordinary minimum-opponent gate rejects a solo VMCE host
  without lobby AI. Admit the supported solo campaign through the authoritative start path.
- **B75 — shared-unit deployment and unloading:** self-click actions and some D-key unloads
  require the primary House despite a shared grant. Reuse the grant for input and retain command
  authorization. See [audit A23](zh-CN/vmce-review.md).
- **B76 — retry-load failure admission:** the failure signal is not tied to an admitted sender,
  VMCE loading phase or attempt and may abort an unrelated network load. Validate all three;
  exercise invalid and stale signals. See [audit A24](zh-CN/vmce-review.md).
- **B77 — load-failure delivery:** the reliable queue is reset immediately after enqueueing a
  local load-failure notice, racing delivery and acknowledgement. Drain it with a bounded failure
  path before cleanup. See [audit A25](zh-CN/vmce-review.md).
- **B79 — campaign AllowWin gate:** VMCE currently follows the network-mode bypass of the
  campaign tag-count gate, potentially ending earlier than the native campaign. Verify the native
  lifecycle and reconcile it with the agreed earliest shared boundary; see [audit A27](zh-CN/vmce-review.md).
- **B81 — cross-platform map paths:** manifest validation normalizes backslashes, but loading may
  treat them literally on Linux/macOS. Normalize consistently and retain path containment checks.
  See [audit A29](zh-CN/vmce-review.md).
- **B82 — equivalent INI sections:** case/separator variants of the same logical stage or House
  control section can pass duplicate checks and silently select the first. Reject ambiguity;
  see [audit A30](zh-CN/vmce-review.md).
- **B83 — construction failure notification:** failed construction can consume the loading
  session before VMCE failure detection, so no notification is queued. Preserve the frozen
  attempt identity; see [audit A31](zh-CN/vmce-review.md).
- **B84 — final installation failure:** a failed network installation shows a local error without
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
- **B91 — synthetic result/defeat capture fixtures:** the `vmce-result-*` and `vmce-defeat-*`
  shell-capture checkpoints fail with `VMCE defeat fixture needs authored role metadata` in this
  build. The lobby fixture caches role details under `game_config.paths.ra2_dir`
  (`install_vmce_lobby_fixture`), while `vmce_map_details` looks the cache up under
  `settings_dir/Maps`; the miss then depends on a second map load that does not resolve in a
  score capture. Fix the fixture cache root/file identity and recapture, then use those shots to
  close the visual part of [A37](zh-CN/vmce-review.md) (defeat retry state) and
  [A38](zh-CN/vmce-review.md) (statistics name truncation).
- **B93 — acceptance runner retail marker:** `vmce_lan_acceptance run` writes the
  `.vera20k-lan-capture-root` marker into each per-process settings directory, but the capture
  path checks the marker under `assets.ra2_dir()`, so a plain `--retail-source` fails with
  `capture writes require an explicitly marked isolated directory`. Write the marker into each
  retail copy (or require and validate a marked source) and document the requirement. The real
  `2p-retry` capture used a previously marked retail copy as a workaround.

## Deferred work

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
- **B80 — in-mission Observe:** the abort dialog may offer Observe and queue local destruction.
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
  B57, B58, B61 and B62 remain open. Locked decisions:
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
- **B15 — host manifest race:** VMCE LAN has a whole-series verification path, but B58 leaves
  distinct Ready/Go boundaries reusing a cached result. The freeze boundary still re-reads
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

