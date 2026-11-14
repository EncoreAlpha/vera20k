//! The lobby's VMCE scenario-catalog refresh.
//!
//! Campaign series reading, identities and next-stage resolution live in
//! `crate::app::vmce_campaign`; this adapter only removes stale VMCE records
//! and inserts the validated discovery into the shared catalog.

use crate::app::AppState;

/// Refresh validated campaign Entry maps in the shared scenario catalog.
/// Each direct child of `settings/Maps/VMCE` is one series campaign; its
/// `campaign.ini` and all manifest-relative maps live in that child.
/// Whether this host may expose VMCE. A validated campaign entry is the
/// existing runtime data owner; the feature gate only decides whether this
/// migration bridge has the reader that can produce it.
pub(crate) fn mode_available(state: &AppState) -> bool {
    state.frontend.scenario_catalog.has_vmce_entries()
}

pub(crate) fn refresh_vmce_entries(state: &mut AppState) {
    {
        let mut records = state.frontend.scenario_catalog.records_mut();
        records.retain(|record| {
            !matches!(
                &record.source,
                crate::map::skirmish_scenarios::SkirmishScenarioSource::VmceSeries(_)
            )
        });
    }
    let discovered = crate::app::vmce_campaign::discover_entries(state);
    let mut records = state.frontend.scenario_catalog.records_mut();
    records.extend(discovered);
}
