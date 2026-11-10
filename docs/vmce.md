# Multiplayer Campaign (VMCE)

VMCE is VERA20K's optional LAN campaign expansion: up to four human players progress
through an authored external campaign (`Maps/VMCE/<series>/`), with native campaign
result pages and deterministic shared missions. It does not load the stock Cooperative
campaign modes and does not read `Coopsave.ini`.

## Build

VMCE is a compile-time capability, like Ares or Phobos:

```sh
cargo build --profile fast --features vmce --bin vera20k
```

Without `--features vmce`, the `Multiplayer Campaign` mode row is absent, `Maps/VMCE/`
is not scanned, and VMCE-only session/LAN fields are compiled out.

Current fork version: `0.2.9-vmce.6`, merged with upstream `v0.2.9`. LAN peers should run
the same VMCE build.

## Status

| Area | State |
|---|---|
| Lobby | Four roles, Owner/Ready/colour board, invalid-campaign blocking/inline errors |
| Campaign flow | `NextScenario`, `EndOfGame`, external series directory, strict Entry-stage validation |
| 4-player LAN | Real four-process acceptance: checksum match, four-row settlement, next mission on every peer |
| Defeat retry | Guest vote → host retries the same frozen stage; genuine VMCE `2p-retry` capture passes |
| Result UI | `0x103` victory board + statistics page; native BC6 defeat statistics; no result-page timers |
| i18n | English and zh-CN real; other shipped languages use English placeholders |

Known gaps live in the [VMCE backlog](vmce-backlog.md): setup atomicity (B03), role-change
sync (B13), observer seats (B12), later-stage validation (B05), HUD/loading UI (B20/B21),
and the deferred `AltNextScenario`/`SkipMapSelect`/cross-mission money work (B23).

## Start here

- [VMCE player and map-author guide](vmce-guide.md) — campaign directories,
  `campaign.ini`, LAN flow, result pages, testing and troubleshooting.
- [VMCE backlog](vmce-backlog.md) — engineering task ledger, locked decisions and residuals.
- [VMCE baseline](vmce-baseline.md) — upstream sync, version and real-capture baseline.
- [Chinese guide](zh-CN/vmce.md).

## Relationship to the stock Cooperative mode

VMCE owns its mode, lobby board, campaign data, result pages and statistics projection.
The shared LAN round protocol (begin/options/ack/go, stage identity, retry-vote primitive)
lives in the common session layer and is wrapped for VMCE by a thin `vmce_round` facade.
VMCE does not fork the stock Cooperative result UI; its defeat page is the native BC6
dialog presented as a statistics view.

## Accepting a build

```sh
python tools/vmce_lan_acceptance.py --suite smoke
```

The tool drives only VMCE checkpoints for VMCE scenarios; classic `lan-live-coop-*`
checkpoints are rejected. Receipts, screenshots and comparison data stay under
`.scratch/` and are not committed.
