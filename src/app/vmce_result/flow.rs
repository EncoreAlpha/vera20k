//! VMCE's result-page projections: the campaign score model's inputs and the
//! shared `0x103` model handoff. The settlement statistics come from
//! [`super::statistics`], VMCE's own role/House owner; no Cooperative result
//! owner is involved.

use crate::app::AppState;

/// The campaign score model for a VMCE victory/defeat page. The settlement
/// rows are projected once by the caller and passed in, so the role metadata
/// and assignment join are not repeated per model value.
pub(crate) fn build_vmce_score_model(
    state: &AppState,
    elapsed_score_ticks: i32,
    rows: &[super::statistics::VmceRoleFacts],
) -> crate::ui::campaign_score_shell::CampaignScoreScreenModel {
    let difficulty = state
        .frontend
        .multiplayer_shell_runtime
        .cooperative_difficulty();
    // Native 0x6C92C1 sums PlayerControl Houses; a VMCE role's primary House is
    // player-controlled through the campaign grant table instead, so read the
    // role rows the settlement projects. A shared or observer House is not a
    // row and must not add its kills.
    let houses = vec![(true, super::statistics::enemy_kills_from(rows))];
    let side = score_side(state);
    crate::app::campaign_score_model::build(
        state,
        elapsed_score_ticks,
        difficulty,
        &state.match_state.map_ranking,
        houses,
        side,
    )
}

pub(crate) fn score_side(state: &AppState) -> i32 {
    let Some(sim) = state
        .match_state
        .sim_runtime
        .runtime()
        .map(|runtime| &runtime.simulation)
    else {
        return 0;
    };
    crate::app::input::commands::preferred_local_owner_name(state)
        .as_deref()
        .and_then(|owner| sim.interner().get(owner))
        .and_then(|owner| sim.houses().get(&owner))
        .map_or(0, |house| house.side_index())
}
