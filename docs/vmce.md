# Multiplayer Campaign (VMCE)

VMCE is VERA20K's optional LAN campaign expansion: up to four human players progress
through an authored external campaign (`Maps/VMCE/<series>/`), with native campaign
result pages and deterministic shared missions. VMCE has no observer seats. It does not load the stock Cooperative
campaign modes and does not read `Coopsave.ini`.

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
| Campaign flow | `NextScenario`, `EndOfGame`, external series directory, strict Entry-stage validation |
| LAN validation | Current build: two- and four-process win and next mission, 300 matching checksums |
| Defeat retry | Current build: guest vote → host retries the same frozen stage, 300 matching checksums |
| Result UI | `0x103` victory board + statistics page; native BC6 defeat statistics; no result-page timers |
| i18n | English and zh-CN real; other shipped languages use English placeholders |

The [VMCE backlog](vmce-backlog.md) separates current defects from the agreed but
not yet implemented redesign of outcomes, intermission role selection and unlocks.

## Start here

- [VMCE player and map-author guide](vmce-guide.md) — campaign directories,
  `campaign.ini`, LAN flow, result pages, testing and troubleshooting.
- [VMCE backlog](vmce-backlog.md) — engineering task ledger, locked decisions and residuals.
- [Chinese backlog](zh-CN/vmce-backlog.md).
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
python -m tools.vmce_lan_acceptance run --suite smoke --exe "<VERA20K_EXECUTABLE>" --retail-source "<RETAIL_DIRECTORY>" --series "<VMCE_SERIES_DIRECTORY>"
```

Each run uses the same series for all selected scenarios; the series must suit every
scenario in the chosen suite. Use `--scenario` with an appropriate series when testing
win or retry separately. Replace the placeholders with local paths.

The tool drives only VMCE checkpoints for VMCE scenarios; classic `lan-live-coop-*`
checkpoints are rejected. Receipts, screenshots and comparison data stay under
`.scratch/` and are not committed.
