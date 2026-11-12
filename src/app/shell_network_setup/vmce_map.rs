//! The lobby's VMCE scenario-catalog refresh.
//!
//! Campaign series reading, identities and next-stage resolution live in
//! `crate::app::vmce_campaign`; this adapter only removes stale VMCE records
//! and inserts the validated discovery into the shared catalog.

use crate::app::AppState;

/// Refresh validated campaign Entry maps in the shared scenario catalog.
/// Each direct child of `settings/Maps/VMCE` is one series campaign; its
/// `campaign.ini` and all manifest-relative maps live in that child.
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
