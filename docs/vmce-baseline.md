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
- settlement agreement: the result record binds the round nonce to a digest of
  the resolved next-stage identity, and the local next-stage unlock is written
  only after every online participant confirms that agreement;
- LAN admission: the host advertises one content digest over the whole VMCE
  series (`campaign.ini` plus every manifest map's path, length and bytes) and
  a guest verifies it in a cancellable worker before reporting its map;
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

The current runner selects `4p-win`, `2p-win`, `2p-retry` and `2p-final`; `--players`
controls 2, 3 or 4 rows for the `4p-*` scenarios. It compares the 300-frame checksum
stream and scenario-specific receipt fields; each run uses a single supplied series,
which must suit the selected scenario. Two-, three- and four-player wins and next
missions, a two-player mixed-outcome advance, a two-player retry and the `2p-final`
scenario pass on the current build, and on this branch each peer's local
`VmceProgress.json` was checked to contain only the designated next stage (`2p-final`
writes none, because the last stage resolves no successor). The final-stage
run reached credits and returned after Escape; the full credits duration was not observed.
Run one scenario per invocation on machines with limited shared GPU memory: an exited
capture process can still hold its device allocation, and a following scenario may fail
inside wgpu with "Out of Memory" although each scenario passes separately. Bundles stay
under `.scratch/`; older pre-merge receipts do not prove the current build.
