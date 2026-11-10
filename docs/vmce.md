# Multiplayer Campaign (VMCE)

VMCE is VERA20K's optional LAN campaign expansion: up to four human players progress
through an authored external campaign (`Maps/VMCE/<series>/`), with per-player mission
outcomes, an intermission room for role reselection, locally persisted stage unlocks and
native campaign result pages on deterministic shared missions. VMCE has no observer seats.
It does not load the stock Cooperative campaign modes and does not read `Coopsave.ini`.

## Build

VMCE is a compile-time capability, like Ares or Phobos:

```sh
python -m tools.cargo_run -- build --profile fast --features vmce --bin vera20k
```

Without `--features vmce`, the `Multiplayer Campaign` mode row is absent, `Maps/VMCE/`
is not scanned, and VMCE-only session/LAN fields are compiled out.

Current fork version: `0.2.9-vmce.6`, merged with upstream `v0.2.9`. LAN peers should run
the same VMCE build.

## Status

| Area | State |
|---|---|
| Lobby | Four roles, Owner/Ready/colour board, invalid-campaign blocking/inline errors |
| Campaign flow | `NextScenario`, `EndOfGame`, external series directory; the selected map's `campaign.ini` stage is validated strictly, and LAN admission verifies every manifest map's bytes |
| Outcomes | Per-player win/loss from the map's actual diplomacy; an eliminated primary House keeps its authorized control and the mission continues until a map outcome trigger |
| Intermission | Everyone returns to the campaign room on the designated next stage, reselects roles and readies; joins/leaves and role-count blocks are enforced |
| Unlocks | The next stage is unlocked locally only after every online participant confirms the round's result and stage identity (nonce-bound digest); loading the next stage is not required |
| LAN validation | Current build: two-, three- and four-process win and next mission, two-player retry, 300 matching checksums; each peer's local unlock file checked |
| Result UI | `0x103` victory board + statistics page; native BC6 defeat statistics; no result-page timers |
| i18n | English and zh-CN real; other shipped languages use English placeholders |

The [VMCE backlog](vmce-backlog.md) lists the remaining defects and the connected
acceptance work (cross-machine sessions, mid-intermission joins, packet loss, B10 delivery).

## Start here

- [VMCE player and map-author guide](vmce-guide.md) — campaign directories,
  `campaign.ini`, LAN flow, result pages, testing and troubleshooting.
- [VMCE backlog](vmce-backlog.md) — engineering task ledger, locked decisions and residuals.
- [Chinese backlog](zh-CN/vmce-backlog.md).
- [VMCE baseline](vmce-baseline.md) — upstream sync, version and real-capture baseline.
- [Chinese guide](zh-CN/vmce.md).

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
Entry is the campaign's last stage). A run uses the same series for every selected scenario,
so pick a series that suits the scenario. On machines with limited shared GPU memory, run
one scenario per invocation: a capture process that just exited can still hold its device
allocation, and an immediately following scenario may fail inside wgpu with "Out of Memory"
(exit 101) although each scenario passes on its own.

The tool drives only VMCE checkpoints for VMCE scenarios; classic `lan-live-coop-*`
checkpoints are rejected. Receipts, screenshots and comparison data stay under
`.scratch/` and are not committed.
