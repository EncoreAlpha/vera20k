<img src="docs/images/new-conscirpt-hero-image.png" alt="VERA20K hero image" width="100%">

<p align="center" dir="ltr">
  <a href="README.sv.md" lang="sv">Svenska</a> · <a href="README.zh-CN.md" lang="zh-CN">简体中文</a> · <a href="README.de.md" lang="de">Deutsch</a> · <a href="README.ar.md" lang="ar" dir="rtl">العربية</a> · <a href="README.es.md" lang="es">Español</a> · <a href="README.ja.md" lang="ja">日本語</a> · <a href="README.ru.md" lang="ru">Русский</a> · <a href="README.th.md" lang="th">ไทย</a> · <a href="README.tr.md" lang="tr">Türkçe</a> · <strong>English</strong>
</p>
<p align="center" dir="ltr">
  <a href="https://github.com/YuriPlanet/vera20k/actions/workflows/macos.yml?query=branch%3Amain" title="Latest macOS library test run (run manually)"><img src="https://github.com/YuriPlanet/vera20k/actions/workflows/macos.yml/badge.svg?branch=main" alt="macOS library tests" height="23" align="middle"></a>
  <a href="https://github.com/YuriPlanet/vera20k/actions/workflows/linux.yml?query=branch%3Amain" title="Latest Linux library test run (run manually)"><img src="https://github.com/YuriPlanet/vera20k/actions/workflows/linux.yml/badge.svg?branch=main" alt="Linux library tests" height="23" align="middle"></a>
  <a href="https://github.com/YuriPlanet/vera20k/actions/workflows/windows.yml?query=branch%3Amain" title="Latest Windows library test run (run manually)"><img src="https://github.com/YuriPlanet/vera20k/actions/workflows/windows.yml/badge.svg?branch=main" alt="Windows library tests" height="23" align="middle"></a>
  <a href="https://discord.gg/kmjRUn5m5F"><img src="https://img.shields.io/badge/Discord-Join-5865F2?style=flat&amp;logo=discord&amp;logoColor=white" alt="Join Discord" height="23" align="middle"></a>
  <img src="https://img.shields.io/badge/QQ_group-1097193563-1EBAFC?style=flat&amp;logo=qq&amp;logoColor=white" alt="QQ group" height="23" align="middle">
</p>

# VERA20K

VERA20K is a community-led rewrite of the original engine, `gamemd.exe`. It uses the original
game files, so you need your own copy of *Command & Conquer: Red Alert 2 — Yuri's Revenge*.

This README is also the player and map-author guide for **VMCE — VERA Multiplayer Campaign
Expansion**, an optional multiplayer campaign capability built into VERA20K. VMCE is not a
runtime INI toggle; it is selected at compile time with the Cargo feature `vmce`.

```bash
# Developer build with VMCE enabled:
cargo build --profile fast --features vmce --bin vera20k

# Minimal release build with VMCE enabled:
cargo build --release --no-default-features --features vmce --bin vera20k
```

Without `--features vmce`, the VMCE mode row is not registered and its campaign directories
are not scanned. The normal skirmish, campaign and original Cooperative modes are unaffected.

<img src="docs/images/vera20k-screenshots.png" alt="VERA20K skirmish setup screen and in-game view" width="100%">

## Compile-time features

Like the optional Ares, Phobos and VeraX capabilities, VMCE is an opt-in build feature. The
default shipping profile enables none of them.

| Feature | Default | Enables |
|---|---|---|
| `vmce` | off | `Multiplayer Campaign` mode, `Maps/VMCE` series discovery, VMCE roles and result flow |
| `ares` | off | Supported Ares extensions |
| `phobos` | off | Supported Phobos extensions |
| `npatch` | off | NPatch extension capability (placeholder) |
| `verax` | off | VERA20K first-party extension chains |
| `npbug` | off | NPExt bug-fix capability |
| `dev-overlays` | on for development | Ctrl+Shift diagnostic overlays and dev save/load |

The release workflow builds with `--no-default-features`. Build a VMCE-enabled release
explicitly when you want to distribute it.

## Install

1. Install a VERA20K build that contains `--features vmce`.
2. Install the original *Yuri's Revenge* files for VERA20K as usual. VMCE does not ship or
   redistribute EA game data.
3. Put every campaign series under:

```text
<VERA20K settings directory>/Maps/VMCE/<series>/
├── campaign.ini
├── ra2md.csf          optional; labels used by this series
├── all01umd.map
├── all02umd.map
└── ...
```

One direct child of `Maps/VMCE` is one series campaign. `campaign.ini`, its maps and its
series CSF stay inside that directory. `Entry`, `Scenario1..ScenarioN` and `NextScenario` are
relative to the series directory — do not repeat the series name in those paths.

VMCE now reads only `Maps/VMCE`. The old `Maps/VeraCampaign` and interim `Maps/Vmce`
directories are not read; move each series directory under `Maps/VMCE/<series>/` once.

Maps must be loose `.map` files. A map cannot be listed by two series. LAN peers must install
the same series directory, `campaign.ini`, series CSF and selected map bytes; VMCE does not
transfer files.

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

For example, `Name:AD03` resolves to `林德拉克计划` from the 林德拉克计划 package CSF. A
missing label falls back to the directory name.

## Start a match

1. All players install the same `Maps/VMCE/<series>/` contents.
2. Enter a LAN room and choose the **Multiplayer Campaign** mode row.
3. The host picks a map; every guest must show the map as available (Ready).
4. Each player confirms a role and readies; the host starts.
5. The mission uses one deterministic shared world. Both peers must see the same opening,
   triggers and result.

VMCE is LAN-only. WOL, CnCNet, online matchmaking, mid-mission joins and mid-mission
reconnects are not supported.

## Campaign roles and Houses

Each player role is a primary House from the map:

- `Player1..PlayerN` in `campaign.ini` select the primary Houses.
- `PlayerCount` is the number of roles for that mission. Do not set it higher than the
  number of players you expect to field.
- A shared House (for example `French House`) is controlled by roles granted through
  `PlayerNControl=yes` in `[Scenario:<map>:<House>]`. Shared Houses do not get their own
  result row.
- `PlayerControl=yes` is the legacy form of `Player1Control=yes`. For multiple controllers,
  write `Player1Control=yes`, `Player2Control=yes`, and so on explicitly.
- A map-authored primary House with `PlayerControl=yes` is not automatically given to a
  VMCE role. Add the matching `PlayerNControl=yes` in `campaign.ini` if that House should be
  player-controlled; otherwise it stays AI.

VMCE does not use skirmish start waypoints to place bases. Map authors must place each role's
base and units, or spawn them with triggers. VMCE does not grant an MCV or construction yard.

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
| `NextScenario` | map path | empty | Next mission after victory; must be listed in `ScenarioCount` |
| `AltNextScenario` | map path | empty | Validated but not executed yet |
| `EndOfGame` | boolean | empty | `yes` ends the campaign: no Continue on the result page |

`[Scenario:<map path>:<House name>]`:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `Player1Control..Player4Control` | boolean | empty | `yes` grants this role control of the shared House |
| `PlayerControl` | boolean | empty | Legacy alias for `Player1Control` when no `PlayerNControl` is written |

Unknown keys, duplicate sections/keys, `PlayerN` beyond `PlayerCount`, empty required values,
and escaping paths are rejected. `campaign.ini` is limited to 64 KiB. Every listed map must
exist locally, even if the current session only plays the first mission.

## Mission rules

- All players run one deterministic world; commands enter the same simulation frame.
- Initial alliances and shared vision come from the map `[Houses] Allies=` graph. Put all role
  Houses into each other's `Allies=` lists.
- Losing one role's primary House does not end the mission automatically. Victory and defeat
  come from map trigger outcome actions.
- Cameras, EVA lines, text and input locks apply to every player. VMCE does not filter
  presentation or input locks per role.
- Shroud reset (action 51) and radar interruption (action 139) are House-private. Write one
  action per role when the whole team should see the effect.
- Difficulty in the LAN lobby selects the trigger difficulty column: Easy/Medium/Hard. It does
  not change unit stats, AI difficulty or economy. The slider is frozen when the game starts.

## Outcome actions

| Action | Name | VMCE behavior |
|---|---|---|
| `1` | Winner is a country | Team victory; the country argument is ignored by VMCE |
| `2` | Loser is a country | Team defeat; the country argument is ignored by VMCE |
| `67` | Declare victory | Immediate team victory |
| `68` | Declare defeat | Immediate team defeat |
| `69` | End mission | Resumes the result timer when a prior result exists; otherwise treats the mission as victory |

Use `67`/`68` for new maps. The country arguments of `1`/`2` do not filter roles in VMCE.

Do not rely on one primary House being wiped out to end the mission. Add explicit trigger
conditions for each role. There is no automatic “all roles failed” fallback.

## Result page and Continue

- The BC6 result page has one row per role primary House and shows mission time.
- On victory with a next mission: the host is ready automatically; each guest confirms ready;
  after everyone is ready only the host can start the next mission.
- The next mission is revalidated against the same `campaign.ini` and role mapping.
- On defeat, `EndOfGame=yes`, or no `NextScenario`, Continue is absent; only Quit is offered.
- VMCE does not write `coopsave.ini` or cross-mission progress.

## Map-author notes

- Give every role primary House a different `Country`. Trigger owners and event/action country
  arguments resolve to a Country, and the first House registered for that Country wins.
- Write one trigger per role when the condition belongs to that role.
- Custom countries can be registered in the map `[Countries]`/`[Colors]`; `Side` must already
  exist in rules `[Sides]`. VMCE does not merge a map-authored `[Sides]`.
- `<Player @ X>` selectors, `<random>`/`<none>` role owners, action 15 `AllowWin` and event 33
  (“selected by player”) are not suitable for VMCE role logic. Use country selectors and
  `67`/`68` instead.
- Reinforcement teams, task forces and base vehicles are not duplicated automatically. Write
  one copy per role when each role needs its own.
- AI-only actions usually do nothing on a role House because VMCE marks role Houses as human.

## Current limitations

- LAN only; no WOL/CnCNet/online.
- No in-mission save/load.
- No AI takeover after a player disconnects; no mid-mission join or host migration.
- No cross-mission money/seed/role reselection or persistent progress.
- `AltNextScenario`, `SkipMapSelect`, `EndOfGame` campaign completion page and failure retry
  are not implemented.
- 3–4 players are structurally supported; the result page native layout is still primarily
  validated for two rows.
- VMCE is an optional feature; a build without `--features vmce` has no VMCE mode row.

## Troubleshooting

- **No Multiplayer Campaign mode row:** build with `--features vmce`.
- **Map missing from the lobby:** check for `Maps/VMCE/<series>/campaign.ini`; `Entry` must be
  listed in `Scenario1..ScenarioN`; the map must be a loose `.map`; see logs beginning
  `ignoring invalid VMCE series`.
- **Guest cannot Ready:** the guest lacks the same series directory, `campaign.ini`, series
  CSF or selected map bytes. VMCE does not transfer files.
- **Role House not found:** `PlayerN` must match a House name in the map `[Houses]`.
- **Triggers never fire for the second role:** give each role House a distinct `Country` and
  write separate triggers.
- **Mission keeps running after a role is wiped out:** expected; VMCE does not auto-defeat a
  role House. Add a `67`/`68` trigger.
- **No base at mission start:** VMCE does not spawn an MCV or construction yard; place or
  trigger it.

## Development

See [AGENTS.md](AGENTS.md) for the engine contribution workflow.

```bash
cargo test -p vera20k --lib --features vmce
cargo test -p vera20k-core --lib --features vmce
cargo build --profile fast --features vmce --bin vera20k
```

The detailed Chinese guide, including the full trigger and action notes, is
[docs/zh-CN/veramultiplayer-campaign-expansion.md](docs/zh-CN/veramultiplayer-campaign-expansion.md).
Other engine documentation: [Ares](docs/ares.md), [Phobos](docs/phobos.md),
[NPatch](docs/npatch.md), [VeraX](docs/verax.md), [engine overview](docs/index.md),
[developer tools](tools/README.md) and [Ghidra workflow](docs/ghidra-workflow.md).

## Contributing

Help is welcome anywhere, and these areas need it most. Each links to its roadmap issue, with
the work in progress and the next step:

- **[Gameplay and combat](https://github.com/YuriPlanet/vera20k/issues/3830):** unit behavior, damage and everyday skirmish reliability.
- **[Performance](https://github.com/YuriPlanet/vera20k/issues/3831):** frame rate and responsiveness in large battles and on older hardware.
- **[Multiplayer](https://github.com/YuriPlanet/vera20k/issues/3832):** joining, starting and keeping online matches going, and co-op reliability.
- **[UI and audio](https://github.com/YuriPlanet/vera20k/issues/3833):** menus, text, translations and sound.
- **[Campaigns](https://github.com/YuriPlanet/vera20k/issues/3834):** missions launch, but playing them to the end still needs work: objectives, triggers, scripted transitions, save/load and moving on to the next mission.
- **[Mod compatibility](https://github.com/YuriPlanet/vera20k/issues/3835):** Ares and Phobos features, without changing the original game's defaults.
- **[Testing](https://github.com/YuriPlanet/vera20k/issues?q=is%3Aopen%20label%3Aparity):** play VERA20K next to the original and report what plays differently.

Contributors can take responsibility for engine areas, with broad freedom to make changes that
move VERA20K forward.

Maintainers will help with review, integration and merging. You'll keep full credit for your
contribution.

Discussion is on [Discord](https://discord.gg/kmjRUn5m5F) and in the QQ group (1097193563).

## Architecture

The engine has three main parts:

- **Simulation:** Owns gameplay state and logic.
- **Rendering:** An absolutely new GPU rendering pipeline using the original game assets, built
  to preserve the original look.
- **Application:** Handles input, menus, audio, and platform integration, connecting the systems.

This separation supports deterministic gameplay.

Arrows show data flow:

![Application sends commands to simulation, which returns world state; application sends draw data to rendering.](docs/images/architecture.png)

All donations received are distributed
to contributors according to the [TIBTRE contribution ledger](https://github.com/YuriPlanet/tibtre-ledger).

Payouts to contributors:

<p><img src="https://img.shields.io/badge/PayPal-003087?style=flat&amp;logo=paypal&amp;logoColor=white" alt="PayPal" height="23"> <img src="https://img.shields.io/badge/China_UnionPay-E21836?style=flat&amp;logo=data:image/svg%2Bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9Ii02IC02IDI2OCAxNzIiPjxwYXRoIGQ9Ik01MC4wNDE2MzEsMC4wMDA2Njc1NjMzMDUgTDExMy44MjAyMTEsMC4wMDA2Njc1NjMzMDUgQzEyMi43MjMxMjcsMC4wMDA2Njc1NjMzMDUgMTI4LjI2MDQzOCw3LjI1NzMxMzQ1IDEyNi4xODM1NTgsMTYuMTg5NzY1NyBMOTYuNDkwMDcxNSwxNDMuNjc1NDE2IEM5NC4zOTQ1MzY1LDE1Mi41NzY3NzcgODUuNDgwNzM5LDE1OS44Mzk2NDEgNzYuNTcxNjA1LDE1OS44Mzk2NDEgTDEyLjc5OTI0MzMsMTU5LjgzOTY0MSBDMy45MDg3NjQsMTU5LjgzOTY0MSAtMS42NDA5ODQ0NCwxNTIuNTc2Nzc3IDAuNDM1ODk2MjA4LDE0My42NzU0MTYgTDMwLjE0MTgxODYsMTYuMTg5NzY1NyBDMzIuMjE4Njk5Myw3LjI1NzMxMzQ1IDQxLjEyNzgzMzMsMC4wMDA2Njc1NjMzMDUgNTAuMDQxNjMxLDAuMDAwNjY3NTYzMzA1IiBmaWxsPSIjRTIxODM2IiBzdHJva2U9IiNmZmYiIHN0cm9rZS13aWR0aD0iOCIgc3Ryb2tlLWxpbmVqb2luPSJyb3VuZCIvPjxwYXRoIGQ9Ik0xMDguNTEzMjgzLDAuMDAwNjY3NTYzMzA1IEwxODEuODU3MDE4LDAuMDAwNjY3NTYzMzA1IEMxOTAuNzU4Mzc4LDAuMDAwNjY3NTYzMzA1IDE4Ni43NDQ1MjcsNy4yNTczMTM0NSAxODQuNjUwNTQ2LDE2LjE4OTc2NTcgTDE1NC45NjE3MjQsMTQzLjY3NTQxNiBDMTUyLjg3ODYyNSwxNTIuNTc2Nzc3IDE1My41Mjg0MjcsMTU5LjgzOTY0MSAxNDQuNjA4NDExLDE1OS44Mzk2NDEgTDcxLjI2NDY3NzUsMTU5LjgzOTY0MSBDNjIuMzQ0NjYxNSwxNTkuODM5NjQxIDU2LjgyNDQ0OTUsMTUyLjU3Njc3NyA1OC45MTk5ODUsMTQzLjY3NTQxNiBMODguNjA3MjUyNSwxNi4xODk3NjU3IEM5MC43MTUyMjQsNy4yNTczMTM0NSA5OS42MDU3MDM1LDAuMDAwNjY3NTYzMzA1IDEwOC41MTMyODMsMC4wMDA2Njc1NjMzMDUiIGZpbGw9IiMwMDQ0N0MiIHN0cm9rZT0iI2ZmZiIgc3Ryb2tlLXdpZHRoPSI4IiBzdHJva2UtbGluZWpvaW49InJvdW5kIi8%2BPHBhdGggZD0iTTE3OC45NDc4MywwLjAwMDY2NzU2MzMwNSBMMjQyLjcyNjQwOSwwLjAwMDY2NzU2MzMwNSBDMjUxLjY0MTc2MiwwLjAwMDY2NzU2MzMwNSAyNTcuMTc5MDc0LDcuMjU3MzEzNDUgMjU1LjA4NTA5MywxNi4xODk3NjU3IEwyMjUuMzk2MjcxLDE0My42NzU0MTYgQzIyMy4zMDA3MzYsMTUyLjU3Njc3NyAyMTQuMzgwNzE5LDE1OS44Mzk2NDEgMjA1LjQ2NjkyMiwxNTkuODM5NjQxIEwxNDEuNzE3ODc5LDE1OS44Mzk2NDEgQzEzMi43OTc4NjMsMTU5LjgzOTY0MSAxMjcuMjY1MjE1LDE1Mi41NzY3NzcgMTI5LjM1NDUzMiwxNDMuNjc1NDE2IEwxNTkuMDQ4MDE3LDE2LjE4OTc2NTcgQzE2MS4xMjQ4OTksNy4yNTczMTM0NSAxNzAuMDI3ODE0LDAuMDAwNjY3NTYzMzA1IDE3OC45NDc4MywwLjAwMDY2NzU2MzMwNSIgZmlsbD0iIzAwN0I4NCIgc3Ryb2tlPSIjZmZmIiBzdHJva2Utd2lkdGg9IjgiIHN0cm9rZS1saW5lam9pbj0icm91bmQiLz48L3N2Zz4%3D" alt="China UnionPay" height="23"> <img src="https://img.shields.io/badge/Alipay-1677FF?style=flat&amp;logo=alipay&amp;logoColor=white" alt="Alipay" height="23"> <img src="https://img.shields.io/badge/Vipps-FF5B24?style=flat&amp;logo=data:image/svg%2Bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjQzLjUgMjEuNSAyNyAyNyI%2BPHBhdGggZD0iTTU3LjMsNDAuNmMzLjcsMCw1LjgtMS44LDcuOC00LjRjMS4xLTEuNCwyLjUtMS43LDMuNS0wLjlzMS4xLDIuMywwLDMuN2MtMi45LDMuOC02LjYsNi4xLTExLjMsNi4xIGMtNS4xLDAtOS42LTIuOC0xMi43LTcuN2MtMC45LTEuMy0wLjctMi43LDAuMy0zLjRzMi41LTAuNCwzLjQsMUM1MC41LDM4LjMsNTMuNSw0MC42LDU3LjMsNDAuNnogTTY0LjIsMjguM2MwLDEuOC0xLjQsMy0zLDMgcy0zLTEuMi0zLTNzMS40LTMsMy0zQzYyLjgsMjUuMyw2NC4yLDI2LjYsNjQuMiwyOC4zeiIgZmlsbD0iI2ZmZiIvPjwvc3ZnPg%3D%3D" alt="Vipps" height="23"> <img src="https://img.shields.io/badge/SEPA-2D4B9A?style=flat&amp;logo=sepa&amp;logoColor=white" alt="SEPA" height="23"> <img src="https://img.shields.io/badge/Bitcoin-F7931A?style=flat&amp;logo=bitcoin&amp;logoColor=white" alt="Bitcoin" height="23"></p>

## Support

<a href="https://paypal.me/vera20k"><img src="https://img.shields.io/badge/Donate_via_PayPal-FFC439?style=flat&amp;logo=paypal&amp;logoColor=003087" alt="Support VERA20K via PayPal" height="23"></a>

## Credits and legal

Thanks to OpenRA, XCC Mixer, the ModEnc wiki, Project Perfect Mod, EA's GPL source release of
Command & Conquer and Red Alert, World-Altering Editor, Final Alert, YRpp, Ares, Phobos and
many others.

Copyright © 2026 Halvor Nikolai Rusten and VERA20K contributors.
Licensed under the [GPLv3](LICENSE-GPL) with [additional terms](LEGAL.md#additional-terms).
This repository contains no game files. Command & Conquer and Red Alert are trademarks of
Electronic Arts Inc., and the screenshots and the app icon, Red Alert 2's red star, show game
art owned by Electronic Arts. VERA20K is not affiliated with or endorsed by Electronic Arts.

**Read the [legal notice](LEGAL.md) before you share, sell or fork VERA20K.**
