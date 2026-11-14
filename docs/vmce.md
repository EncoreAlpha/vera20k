# Multiplayer Campaign (VMCE)

VMCE is VERA20K's LAN campaign expansion: up to four human players progress
through an authored external campaign (`Maps/VMCE/<series>/`), with per-player mission
outcomes, an intermission room for role reselection, locally persisted stage unlocks and
native campaign result pages on deterministic shared missions. VMCE has no observer seats.
It does not load the stock Cooperative campaign modes and does not read `Coopsave.ini`.

## Build and activation

The current `feature/vmce` tree still uses the `vmce` Cargo feature for the remaining VMCE reader and gameplay backend:

```sh
python -m tools.cargo_run -- build --profile fast --features vmce --bin vera20k
```

The candidate first slice is now compiled in every build: the `Multiplayer Campaign` mode ID,
the `VmceSeries` map-source filter, and guest option parsing exist without the feature. A
feature-off row stays hidden, does not scan `Maps/VMCE/`, and rejects a VMCE LAN host setup
before changing the shared session. A feature-on build keeps the existing setup, map/config/role,
seed, loading and gameplay behavior. No wire format changed.

Runtime availability for this slice is owned by the validated VMCE campaign records already in
`ScenarioCatalog`; the mode row alone is not a start permit. The feature still selects the
remaining VMCE backend while that full chain is migrated. No new INI key or `RuleSet` switch was
introduced. See the authoritative [Chinese roadmap section](zh-CN/vmce-roadmap.md#取消-vmce-游戏行为的编译期门控).

Current fork version: `0.3.3`, synced to upstream main `0f7dc1bbc` (merge
commit `78b0aef4a`). LAN peers should run the same VMCE build. The fork baseline and what the merge
changed are in the [VMCE baseline](vmce-baseline.md); the 0.3.3 LAN acceptance has been
re-run (2p-win/2p-retry/2p-final/4p-win), and the earlier 0.3.2 screenshot matrix and stock
Cooperative continuation remain recorded there.

## Status

| Area | State |
|---|---|
| Lobby | Four roles, Owner/Ready/colour board, invalid-campaign blocking/inline errors |
| Campaign flow | `NextScenario`, `EndOfGame`, external series directory; the selected map's `campaign.ini` stage is validated strictly, and LAN admission verifies every manifest map's bytes |
| Outcomes | Per-player win/loss from the map's actual diplomacy; an eliminated primary House keeps its authorized control and the mission continues until a map outcome trigger |
| Intermission | Everyone returns to the campaign room on the designated next stage, reselects roles and readies; joins/leaves and role-count blocks are enforced |
| Unlocks | The result and stage identity must be confirmed; the unlock is saved immediately on that agreement, before Continue or the intermission room (B57 closed, `9de82144e`). Loading the next stage is not required |
| LAN validation | Validated on the merged upstream-`0f7dc1bbc` tree (`vmce-upstream-0f7dc1bbc-r1`): two-process win/next mission, retry and campaign end plus four-process win, 300 matching checksums each; the earlier merged-0.3.2 and upstream-867ae2467 matrices remain in the baseline page |
| Module ownership | B5 (2026-11-12) moved VMCE wire records, the UDP intermission transport and session helpers into VMCE-owned modules. The gate count is not a completion metric: a 2026-11-14 static inventory found 757 `cfg(feature = "vmce")`/`cfg!` occurrences across 73 source files, including tests and diagnostics. The current tree is still compile-gated; the runtime-activation migration has not been implemented |
| Result UI | `0x103` victory board + statistics page; native BC6 defeat statistics; no result-page timers. Authored at 800x600, fitted to the 1024x768 default window; both dialogs verified at 800x600, 1024x768 and 1600x900 |
| i18n | The ten older catalogs carry the 32 VMCE keys: English and zh-CN real, the others English placeholders. The Vietnamese catalog now carries the same keys as English placeholders added by the upstream sync; translation remains open (B101) |

## Known open findings

The authoritative priority list is the [Chinese consolidated review](zh-CN/vmce-review.md), currently
55 entries (45 `A` findings plus 10 B-only residuals), plus the post-0.3.2 baseline items
B101–B105 in the [backlog](vmce-backlog.md) (B103/B105 closed). B40/B41 and the connected-validation
items remain open; B106 (campaign data owner) and B10 (terminal LAN delivery) are closed, so LAN and
the result flow no longer read campaign data through the lobby and terminal handoffs fail locally
instead of waiting forever. Player-visible gaps that matter to this overview:

- **A44/B99:** an intermission-room guest can miss the next-stage map preview and the "map has a
  briefing" chat hint; the host shows the preview when its local map carries decodable preview
  data, which was captured with a preview-capable series map.
- **A45/B100:** a full 2/2 or 4/4 VERA intermission room is still advertised as joinable in the LAN
  browser; the join ends in `TXT_GAME_FULL`.
- **A38/B90:** the victory/campaign-end statistics page does not ellipsize long player names the
  way the result board and defeat page do.
- **A41/B95:** map cinematics (`Intro`/`Brief`/`Action`/`Win`/`Lose`/`PostScore`/`PreMapSelect`)
  are not played on the VMCE mission-start and continuation paths.

The synthetic result/defeat capture fixtures need the B91 workaround recorded in the backlog;
that is a tooling issue, not a player-facing defect. The [VMCE backlog](vmce-backlog.md) also
tracks connected acceptance work (cross-machine sessions, mid-intermission joins, live
packet-loss capture) and implementation residuals.

## Start here

- [VMCE player and map-author guide](vmce-guide.md) — campaign directories,
  `campaign.ini`, LAN flow, result pages, testing and troubleshooting.
- [VMCE backlog](vmce-backlog.md) — engineering task ledger, locked decisions and residuals.
- [VMCE roadmap](zh-CN/vmce-roadmap.md) — the authoritative staged plan for removing game-behavior compile-time gates while retaining the independent branch.
- [Chinese consolidated review](zh-CN/vmce-review.md) — merged findings ordered by fix priority.
- [VMCE baseline](vmce-baseline.md) — upstream sync, version and real-capture baseline.
- [Chinese hub](zh-CN/vmce.md).

## Relationship to the stock Cooperative mode

VMCE owns its mode, lobby board, campaign data, result pages and statistics projection.
The shared LAN round protocol (begin/options/ack/go, stage identity, nonce-bound result and
next-stage agreement, retry-vote primitive) lives in the common session layer and is wrapped
for VMCE by a thin `vmce_round` facade; VMCE's LAN service owns the whole-series admission
verification that gates a guest's map answer. VMCE does not fork the stock Cooperative result
UI; its defeat page is the native BC6 dialog presented as a statistics view.

## Accepting a build

```sh
python -m tools.vmce_lan_acceptance run --exe "<VERA20K_EXECUTABLE>" --retail-source "<RETAIL_DIRECTORY>" --series "<VMCE_SERIES_DIRECTORY>" --scenario 2p-win --players 2
```

`--players` selects 2, 3 or 4 rows for the `4p-*` scenarios; `--scenario` selects
`4p-win`, `2p-win`, `2p-retry` or `2p-final` (the final-stage scenario needs a series whose
Entry is the campaign's last stage). `--entry` selects the map the run starts from (default
`stage01/all01umd.map`); for a series that does not start at the default first stage (such as a
`2p-final` last-stage series) pass the last-stage path and keep `campaign.ini`'s `Entry` on the
same stage. A run uses the same series for every selected scenario,
so pick a series that suits the scenario. On machines with limited shared GPU memory, run
one scenario per invocation: a capture process that just exited can still hold its device
allocation, and an immediately following scenario may fail inside wgpu with "Out of Memory"
(exit 101) although each scenario passes on its own.

The tool drives only VMCE checkpoints for VMCE scenarios; classic `lan-live-coop-*`
checkpoints are rejected. Receipts, screenshots and comparison data stay under
`.scratch/` and are not committed.
