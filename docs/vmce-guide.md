---
layout: default
title: VERA Multiplayer Campaign Expansion
---

# VERA Multiplayer Campaign Expansion (VMCE)

VMCE is an optional multiplayer campaign capability built into VERA20K. It lets up to four players
play a cooperative campaign described by an external `campaign.ini`, on top of the original
*Yuri's Revenge* assets. VMCE is not a runtime INI toggle: it is selected at compile time with
the Cargo feature `vmce`.

VMCE does not support observers. If a room already has observers, they must become
players or leave before the host selects a VMCE campaign.

Win captures wait for the result page's entry animation before checking its board and clicking
Ready and Continue. Random roles display the House assigned when the mission started.

This page is the player and map-author guide. The Chinese version is
[zh-CN/vmce-guide.md](zh-CN/vmce-guide.md).

This guide describes the current working tree. Per-player outcomes, the intermission room and
local unlocks are implemented and have focused plus local multi-process coverage; the backlog
lists the connected (cross-machine) scenarios that have not yet been exercised.

## Relation to the original

The original YR campaign is a single player tracking one current side; missions are chained by
campaign saves. The original Cooperative mode is driven by `CoopCampMD` and the BC6 result
dialog, advances through campaign missions, and writes `coopsave.ini` at mission end.

VMCE takes another path:

- The campaign directory, mission order and per-mission roles come from an external
  `campaign.ini`, not from `CoopCampMD`.
- Each player role is a primary House authored by the map; there is no skirmish-style
  "spawn at player start position" assignment.
- Victory and defeat come only from trigger outcome actions; losing one role's primary House
  does not end the mission automatically.
- Role and non-role Houses keep campaign semantics and are not auto-defeated merely because
  they have no units.
- Inside a mission, random crates and ShortGame follow campaign semantics, and the
  Special House auto-alliance and Secret Lab random assignment are skipped. The matching
  lobby switches are shown disabled for VMCE.
- Victory and campaign completion use the campaign score page `0x103`; defeat uses the BC6
  statistics page. Guests ready for the next mission or vote to retry, and the host continues
  after the corresponding gate opens. VMCE does not write `coopsave.ini` or save progress.

VMCE does not change the original single-player campaign or ordinary skirmish rules. Those
modes keep their original behavior and do not read the VMCE campaign format.

## Install and launch

### Prerequisites

0. Install a working VERA20K build, plus the original *Yuri's Revenge* rules, maps and art.
   VMCE does not ship or redistribute any EA game data.
1. VMCE needs no Syringe, `Phobos.dll` or `Ares.dll`. Do not configure it as a Phobos plugin.
2. Every player must read the same original assets. VMCE exchanges only campaign and map
   **identity**, never files.

### Build feature

VMCE is a compile-time capability like Ares, Phobos and VeraX, and is not part of the default
or shipping profile:

```bash
# Developer build with VMCE enabled:
python -m tools.cargo_run -- build --profile fast --features vmce --bin vera20k

# Minimal release build with VMCE enabled:
python -m tools.cargo_run -- build --release --no-default-features --features vmce --bin vera20k
```

Without `--features vmce`, the `Multiplayer Campaign` mode row is not registered and VMCE
campaign directories are not scanned. VMCE-only session state, LAN round/options fields and
lobby helpers are feature-gated too, so such a build carries none of them. This fork reports its version as
`0.2.9-vmce.N` (currently `0.2.9-vmce.6`, merged with upstream `v0.2.9`); LAN peers should
run the same VMCE build.

### Campaign directories

```text
<VERA20K settings directory>/Maps/VMCE/<series>/
├── campaign.ini
├── ra2md.csf          optional; labels used by this series
├── all01umd.map
├── all02umd.map
└── ...
```

One direct child of `Maps/VMCE` is one series campaign. `campaign.ini`, its maps and its series
CSF stay inside that directory. `Entry`, `Scenario1..ScenarioN` and `NextScenario` are relative
to the series directory — do not repeat the series name in those paths.

VMCE reads only `Maps/VMCE`. The old `Maps/VeraCampaign` and interim `Maps/Vmce` directories
are not read; move each series directory under `Maps/VMCE/<series>/` once.

Maps must be loose `.map` files, and one map cannot be listed by two series. LAN peers must
install the same complete series directory: the host advertises one content digest over
`campaign.ini`, the manifest order, and every manifest map's relative path, byte length and raw
bytes, and a guest may report its map as available only after its own copy produces that digest.
VMCE never transfers files. The optional series CSF is not part of the digest, so distribute the
same file to keep labels consistent.

### Series display name from CSF

`[Campaign] Name=` is optional. It is a CSF label, not literal text:

```ini
[Campaign]
Name=Name:AD03
Entry=all01umd.map
ScenarioCount=2
Scenario1=all01umd.map
Scenario2=all02umd.map
```

Resolution order:

1. the series directory's own `ra2md.csf`;
2. the base game CSF;
3. the series directory name.

For example, `Name:AD03` resolves to the package CSF's display name. A missing label falls back
to the directory name.

## Start a match

1. All players install the same `Maps/VMCE/<series>/` contents.
2. Enter a LAN room and choose the **Multiplayer Campaign** mode row.
3. The host picks a map; every guest must show the map as available (Ready).
4. Each player confirms a role and readies; the host starts.
5. For a solo check, the host may add an AI seat like Skirmish. The AI does not take a
   campaign role; on maps that ignore global AI triggers it may remain inactive.
6. The mission uses one deterministic shared world. Both peers must see the same opening,
   triggers and result.

VMCE is LAN-only. WOL, CnCNet, online matchmaking, mid-mission joins and mid-mission
reconnects are not supported.

## Campaign roles and Houses

Each player role is a primary House from the map:

- `Player1..PlayerN` in `campaign.ini` select the primary Houses.
- `PlayerCount` is the number of roles for that mission. Do not set it higher than the number
  of players you expect to field.
- A shared House (for example `French House`) is controlled by roles granted through
  `PlayerNControl=yes` in `[Scenario:<map>:<House>]`. Shared Houses do not get their own
  result row.
- `PlayerControl=yes` is the legacy form of `Player1Control=yes`. For multiple controllers,
  write `Player1Control=yes`, `Player2Control=yes`, and so on explicitly.
- A map-authored primary House with `PlayerControl=yes` is not automatically given to a VMCE
  role. Add the matching `PlayerNControl=yes` in `campaign.ini` if that House should be
  player-controlled; otherwise it stays AI.

VMCE does not use skirmish start waypoints to place bases. Map authors must place each role's
base and units, or spawn them with triggers. VMCE does not grant an MCV or construction yard.

## Mission rules

- All players run one deterministic world; commands enter the same simulation frame.
- Initial alliances and shared vision come from the map `[Houses] Allies=` graph. Put all role
  Houses into each other's `Allies=` lists.
- Losing one role's primary House does not end the mission or cause defeat effects. The player
  keeps authorized shared control and normal fog, and can control newly spawned own units.
  Map triggers can change diplomacy; players cannot change it manually.
- Cameras, EVA lines, text and input locks apply to every player. VMCE does not filter
  presentation or input locks per role.
- Shroud reset (action 51) and radar interruption (action 139) are House-private. Write one
  action per role when the whole team should see the effect.
- Difficulty in the LAN lobby selects the trigger difficulty column: Easy/Medium/Hard. It does
  not change unit stats, AI difficulty or economy. The row is frozen when the game starts and
  is carried into follow-up missions.

## Result page and Continue

VMCE victory and the campaign-end/last mission share the original campaign score page `0x103`
(layout and input); the defeat result is the native BC6 cooperative statistics dialog instead.
The result state, Continue and retry semantics belong to VMCE.

- Victory/campaign-end `0x103`: par time, your time, rank and commendation stay native; the
  top report's destroyed-enemies label/value are hidden and the native panel keeps its height.
  The page shows a `玩家 | 所属方 | 国家 | 结果 | 准备` board for up to four roles with the
  lobby player name, map House name and role House colour (no row backdrop strips).
- Right panel: row one `Continue` for host and guest, row two Statistics, bottom Leave Room.
  Guest Continue records readiness once; host Continue requires every connected guest ready. The board and
  button states update with the round's Ready changes.
- The `统计数据` button opens a full-screen page over the BC6-style background: total and par
  time in the header, then four role rows and six columns `玩家 | 击毁 | 损失 | 建造 | 分数 |
  收入`. Income is gross credits earned (positive credit gains only; no spent/net), a
  display-only statistic. The statistics page's right panel has only a normal bottom `返回`;
  there is no left tab, no bottom Leave and no other button. The page is victory/campaign-end
  only; the defeat BC6 page is itself the statistics view.
- Victory with a next mission: guests ready on the result page; the host continues to the
  fixed next-stage room. Players select roles and ready again before the host starts.
- Campaign end (`EndOfGame=yes`): the same all-ready gate; every peer plays the campaign
  completion locally and returns to the main menu. There is no 30 s forced start or host pull:
  an unready guest stalls the page, and the only exit is Leave Room.
- Defeat: the page is the native BC6 cooperative result dialog, opened as the statistics view
  itself (time header plus the shared six-column table `Player / Kills / Losses / Built / Score /
  Income`; rows match admitted players only — 2p shows 2, 4p shows 4 — with lobby names, and
  chat/Ready/stats entry hidden). Each guest's `重试` click casts one retry vote and immediately
  lights the native button; the host's `重试` starts disabled and lights only after every online
  guest voted, then restarts the same frozen stage with the same seed, roles and map — no timeout
  and no host vote. `Quit` becomes `离开房间`/Leave Room (host disbands, guest exits locally).
  The genuine VMCE 2p-retry capture passes: the guest votes, the host retries, both reload the
  same VMCE stage, 300 native checksums match, and host/guest table stats are identical.
  Defeat statistics use each player's frozen House assignment, including when command-source
  order differs from seat order.
- A stage with neither `NextScenario` nor `EndOfGame=yes` is an invalid campaign: VMCE
  rejects it at host setup instead of guessing that the campaign ended. Admission verifies every
  manifest map's bytes, but only the Entry stage's campaign configuration is validated at setup;
  a malformed later stage's configuration fails at the transition (B05).
- Before a next mission loads, peers exchange the identity they resolved (map path plus
  source, effective-map and config digests). A mismatch rejects the transition and requests a
  return to the lobby. The rejection can currently be lost during transport teardown (B10),
  leaving a peer waiting; two-sided recovery is not yet guaranteed.
- On load, roles are re-resolved against the next map's own `[Basic] PlayerN`/`[Houses]`
  binding; the previous mission's House names are not carried over. The next mission's seed is
  derived deterministically from the previous seed and the next stage's identity (both peers
  compute it locally); the frozen difficulty row carries over.
- The result page does not write `coopsave.ini`. The next stage is unlocked locally only after
  every online participant confirms the round's result and next-stage identity: the result record
  binds the round's nonce to a digest of the resolved stage, so opening the page alone writes
  nothing, and a peer whose result or next stage differed never confirms. Loading the next stage
  is not required. A verified intermission guest unlocks the designated stage without receiving
  earlier stages. Admission verifies the whole series (see the install section) before the host
  can start; the selected map and `campaign.ini` are also checked individually.

Cross-mission money, `AltNextScenario` execution and `SkipMapSelect` branches are still
not implemented; see Current limitations.

## campaign.ini reference

`[Campaign]`:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `Name` | CSF label | series directory | Series name shown in the lobby |
| `Entry` | map path | required | First mission; must be listed in `Scenario1..ScenarioCount`, relative to the series directory |
| `ScenarioCount` | integer `1..512` | required | Number of listed missions |
| `Scenario1..ScenarioN` | map path | required | Loose map path relative to the series directory; no duplicates |

`[Scenario:<map path>]`:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `PlayerCount` | integer `1..4` | `0` (invalid) | Number of player roles in this mission |
| `Player1..PlayerN` | House name | required | Must exist in the map `[Houses]` registry; role Houses must be distinct |
| `NextScenario` | map path | empty | Next mission after victory; must be listed in `ScenarioCount`; ignored when `EndOfGame=yes` |
| `AltNextScenario` | map path | empty | Validated but not executed yet |
| `EndOfGame` | boolean | empty | `yes` makes this the campaign's last mission and wins over `NextScenario`; the Entry stage is checked at host setup, later stages at transition |

`[Scenario:<map path>:<House name>]`:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `Player1Control..Player4Control` | boolean | empty | `yes` grants this role control of the shared House |
| `PlayerControl` | boolean | empty | Legacy alias for `Player1Control` when `Player1Control` is absent |

A `[Scenario:...]` section matches the map path case-insensitively and treats `/` and `\` as
equivalent, so the section header does not have to repeat `ScenarioN`'s exact spelling. The
same rule applies to the House name in a control section. Unknown keys, duplicate
sections/keys, `PlayerN` beyond `PlayerCount`, empty required values, and escaping paths are
rejected. `campaign.ini` is limited to 64 KiB. Every listed map must exist locally, even if the
current session only plays the first mission.

## Outcome actions

This table describes the working tree; full multiplayer validation is still in progress.

| Action | Name | VMCE behavior |
|---|---|---|
| `1` | Winner is a country | Allies of the first House matching the country win; other players lose, including for an AI House |
| `2` | Loser is a country | Players matching the country lose; other players win |
| `67` | Declare victory | Immediate team victory |
| `68` | Declare defeat | Immediate team defeat |
| `69` | End mission | Resumes the result timer when a prior result exists; otherwise treats the mission as victory |

Actions `67`/`68` set the same outcome for every role; use the table when designing mixed outcomes.

Do not rely on one primary House being wiped out to end the mission. Add explicit trigger
conditions for each role. There is no automatic "all roles failed" fallback.

## Campaign redesign — validation in progress

The accepted direction is one integrated feature covering per-player outcomes, surviving
participation, intermission role selection and locally persistent stage unlocks. Its detailed
requirements and validation work are in the [VMCE backlog](vmce-backlog.md).

- A country-index victory should make its allies winners and non-allied players losers,
  including when an AI House triggers it. The trigger timing, competing outcomes and defeat
  action behavior require original-engine investigation; they are not yet established here.
  Map triggers, not players, change diplomacy during a mission.
- Players whose primary House is eliminated retain authorized shared control and normal fog;
  map-spawned units belonging to that House become controllable again. Even if all primary
  Houses are eliminated, the mission waits for the map's outcome actions. `PlayerNControl`
  grants remain in force when diplomacy changes; connected validation is still needed.
- Mixed winners and losers share one result and continuation flow, with each player's outcome
  shown and no separate elimination marker. A previously eliminated ally of the winner also
  wins. If at least one player wins, everyone may continue; if all lose, guests vote to retry
  and the host starts once they have voted, using the same frozen roles and seed.
- After the result ready gate, players return to the campaign room with the designated next stage,
  reselect roles and ready again. Joining and leaving are allowed between stages. If there are
  more players than roles, keep the room and block Start until the count is valid. The host
  cannot replace the designated stage within that intermission.
- Completing a stage and resolving the next stage unlocks it locally for its participants once
  every online participant confirmed the result and the stage identity, without requiring the next
  stage to load. A verified intermission guest unlocks the designated next stage, without
  receiving earlier stages; this includes returning disconnected players. Unlocks survive content
  updates under the same campaign directory and stage path; persistence is implemented in the
  working tree, while real cross-machine guest joins still need connected validation.
  New hosts may select their locally unlocked stages; guests need matching files, not prior unlocks.
- The final stage uses the common result gate, followed by the completion presentation and a
  return to the main menu. No observers are introduced. Cross-mission money and branching keys
  remain separate deferred work; if branches are added, only the actually chosen branch unlocks.

## Map-author notes

- Give every role primary House a different `Country`. Trigger owners and event/action country
  arguments resolve to a Country, and the first House registered for that Country wins.
- Write one trigger per role when the condition belongs to that role.
- `[Ranking] ParTimeEasy/Medium/Hard` and the matching `UnderParTitle`/`UnderParMessage` or
  `OverParTitle`/`OverParMessage` feed the victory page. Missing fields use the native
  defaults; the page is still shown.
- Custom countries can be registered in the map `[Countries]`/`[Colors]`; `Side` must already
  exist in rules `[Sides]`. VMCE does not merge a map-authored `[Sides]`.
- A map `[Colors]` entry can add a color scheme; if it reuses a rules name, VERA currently
  updates its values (the original engine does not update an existing scheme — that difference
  is still open).
- `<Player @ X>` selectors, `<random>`/`<none>` role owners, action 15 `AllowWin` and event 33
  ("selected by player") are not suitable for VMCE role logic. Use country selectors and
  `67`/`68` instead.
- Reinforcement teams, task forces and base vehicles are not duplicated automatically. Write
  one copy per role when each role needs its own.
- AI-only actions usually do nothing on a role House because VMCE marks role Houses as human.

## Automated acceptance

After an upstream sync, see [VMCE fork baseline](vmce-baseline.md) for the
merge/version/validation state. `tools/vmce_lan_acceptance.py` runs the
real-process captures:

- `--suite dev`: one short `2p-retry` flow probe. It caps each capture at 120 s
  and stops as soon as either peer exits non-zero, so a development probe
  cannot idle for the release timeout.
- `--suite fast`: `4p-win`; `--suite smoke` and `--suite full`: `4p-win`,
  `2p-win` and `2p-retry`, with the full 300-frame checksum comparison.
- `VMCE_CAPTURE_TIMEOUT=<seconds>` overrides the per-capture cap (default
  300 s outside `dev`).
- `compare` runs offline over receipts; the `run` command only launches the
  processes and keeps the bundles under `.scratch/`.
- Run one scenario per invocation on machines with limited shared GPU memory:
  a capture process that just exited can still hold its device allocation, and
  an immediately following scenario may then fail inside wgpu with "Out of
  Memory" (exit 101) although every scenario passes when run separately.

## Current limitations

- LAN only; no WOL/CnCNet/online.
- No in-mission save/load.
- No AI takeover after a player disconnects; no mid-mission join or host migration.
- Cross-mission money is not carried over. Role reselection, whole-series admission verification
  and local stage unlocks are implemented; real cross-machine guest and failure paths still need
  connected validation.
- The working tree has passed two-, three- and four-process win → next mission captures with
  300 matching checksums, and each peer's local unlock file contains only the designated next
  stage. They exercise the result board, Ready/Continue controls, nonce-bound result agreement
  and whole-series admission; cross-machine play and changing a guest's role after joining have
  not been covered.
- A real two-process `EndOfGame` probe reached the credits on both peers, then used Escape
  to return both to the main menu. It compared 300 checksums; it did not watch the entire
  credits roll. The standard acceptance runner's `2p-final` scenario passes on the current build
  (credits and campaign-end screenshots on both peers). VMCE `2p-retry`
  capture passed (BC6 page, guest votes → host retry → same stage, 300 matching checksums).
  There are no result-page timers anywhere: defeat
  uses the vote gate and campaign end the all-ready gate (B02 cancelled). Version
  `0.2.9-vmce.6` is the current fork release.
- The VMCE lobby board is `玩家 | 所属方 | 国家 | 准备 | 颜色`; the settlement page has the
  five-column result board, the right-panel statistics button/page, and the BC6 defeat page.
  HUD and loading/transition UI are deferred.
- Unclaimed role rows show the native `Waiting` text in the country cell and a muted grey swatch;
  the Owner cell names the whole authored House, and a room problem (role conflict, missing or
  mismatched campaign) appears in the full-width warning strip in orange rather than yellow.
- The lobby lists only the selected map's actual primary roles: the player rows, Ready/colour
  slots, the role board and the host's room limit all follow that count (2–4), so a three-role map
  shows three seats instead of four.
- i18n policy: English and zh-CN have real VMCE copy; the other eight shipped languages keep
  English placeholders and are translated later (B08). VERA labels default to the Windows system
  language when the player has not picked one; native CSF follows the install language, and that
  mismatch is expected.
- Known gaps and their evidence live in [the VMCE backlog](vmce-backlog.md).
- `AltNextScenario` and `SkipMapSelect` execution are not implemented. Cross-mission money
  is not implemented either.
- Pause follows native multiplayer: ESC opens a local menu, the modal pump keeps
  `Main_Tick` running (`0x0055DEE0` -> `0x00647040`; `0x00623120` calls `0x0055D360` in
  network modes), and there is no pause packet or all-player pause. VERA's LAN/VMCE
  simulation keeps running behind the menu; only the local input is held.
- VMCE is an optional feature; a build without `--features vmce` has no VMCE mode row.

## Troubleshooting

- **No Multiplayer Campaign mode row:** build with `--features vmce`.
- **Map missing from the lobby:** check for `Maps/VMCE/<series>/campaign.ini`; `Entry` must be
  listed in `Scenario1..ScenarioN`; the map must be a loose `.map`; check log lines beginning
  `ignoring invalid VMCE series`.
- **Campaign is missing `[Scenario:...]`:** the section does not name the selected map path.
  `/` and `\` are interchangeable and ASCII case is ignored, but the map must exist on disk.
- **Guest cannot Ready:** the guest lacks the same series directory, `campaign.ini`, selected map
  bytes, or differs in another map of the series. Admission hashes the whole series (every
  manifest map's path, length and bytes plus `campaign.ini`) in a background scan; a pending scan
  also keeps Ready disabled. The series CSF is not hashed, and VMCE does not transfer files.
- **Role House not found:** `PlayerN` must match a House name in the map `[Houses]`.
- **Triggers never fire for the second role:** give each role House a distinct `Country` and
  write separate triggers.
- **Mission keeps running after a role is wiped out:** expected; loss of a role's units does not
  end a VMCE mission. Add the desired outcome trigger when the mission should end.
- **No base at mission start:** VMCE does not spawn an MCV or construction yard; place or
  trigger it.
