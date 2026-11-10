# VMCE fork baseline

This branch tracks upstream `vera20k` and carries the optional VMCE
(`--features vmce`) campaign expansion. The baseline below is the state after
the upstream sync, not a native-parity certification.

| Item | Value |
| --- | --- |
| Upstream baseline | `38630a77b` (`Fetch the browser build's fonts other than Noto Sans when text needs them`, #4131) |
| Merge commit | `66e7a1b6b` (`Merge upstream/main into the VMCE fork`) |
| Fork version | `0.2.9-vmce.6` (all three workspace manifests and `Cargo.lock`) |
| Capability | `vmce` (off by default; shipping builds do not enable it) |

## Resolution policy

Shared files follow upstream, then replay the VMCE work. The VMCE-only
surfaces that survive the merge:

- cooperative rounds: VERA's Next / CampaignEnd / Retry endings and ready
  gates. There is no retry grace timer or forced campaign-end pull;
- campaign score `0x103`: upstream Smooth Online Menus slide path plus the
  VERA settlement board; the native Continue rect/input is unchanged;
- VMCE lobby: the flat native board, the setup 90-tick/high-credit presets
  from upstream and the VMCE-only role/country/color columns;
- `campaign.ini`: upstream Ares INI syntax (`+=`, `[Child]:[Base]`,
  later-wins) plus VERA's external empty-value overlay parser;
- Esc: upstream's settings dialog owns the key; VMCE lobby and settlement
  screens do not intercept Esc.

## Validation

The fork gate (`.github/workflows/vmce-tests.yml`) runs the upstream-registered
capability build and `python -m tools.capability_tests focus vmce` on Linux.
Locally the same build/test pairs are run for the four configurations
(default, `vmce`, `ares`, `vmce,ares`), together with the conflict-area tests
(Esc/settings input, setup presets, `0x103` renderer/layout, INI parser, house
defeat) and the i18n completeness suite.

The shipped i18n baseline keeps every language key-complete: English carries
the real VMCE labels, zh-CN keeps the fork's translations, and the other eight
language files use the English text as explicit placeholders until translated.

## Real-process captures

The current runner's `smoke` suite lists `4p-win`, `2p-win` and `2p-retry`.
It compares the 300-frame checksum stream and scenario-specific receipt fields;
each run uses a single supplied series, which must suit the selected scenarios.
Two- and four-player wins and a two-player retry have been exercised with their
appropriate series. A separate two-process final-stage probe reached credits and
returned after Escape; the standard runner does not yet implement `2p-final` and
the full credits duration was not observed. Bundles stay under `.scratch/`;
older pre-merge receipts do not prove the current build.
