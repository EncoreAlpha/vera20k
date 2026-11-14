//! VMCE's result-page projections: the campaign score model's inputs and the
//! shared `0x103` model handoff. The settlement statistics come from
//! [`super::statistics`], VMCE's own role/House owner; no Cooperative result
//! owner is involved.

use crate::app::AppState;
use crate::skirmish_launch::LobbyDifficulty;

/// The difficulty row the finished VMCE mission ran at.
///
/// The start froze the shared lobby row into the mission's assignments, and the
/// loader used that same copy for TriggerType's difficulty flags. Reading it
/// here keeps one authority for the mission: a later lobby edit, a rebuilt room
/// or the intermission's next-stage row cannot relabel a finished mission.
/// `None` is a result page without a frozen VMCE start; the default row keeps
/// the page buildable without inventing a second lobby read.
fn result_difficulty(assignments: Option<&crate::skirmish_launch::VmceHouseAssignments>) -> i32 {
    assignments.map_or(LobbyDifficulty::NORMAL.native_row(), |assignments| {
        LobbyDifficulty::from_native_row(assignments.difficulty).native_row()
    })
}

/// The campaign score model for a VMCE victory/defeat page. The settlement
/// rows are projected once by the caller and passed in, so the role metadata
/// and assignment join are not repeated per model value.
pub(crate) fn build_vmce_score_model(
    state: &AppState,
    elapsed_score_ticks: i32,
    rows: &[super::statistics::VmceRoleFacts],
) -> crate::ui::campaign_score_shell::CampaignScoreScreenModel {
    let difficulty = result_difficulty(state.match_state.session.network_vmce_assignments());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish_launch::VmceHouseAssignments;

    #[test]
    fn the_result_page_reports_the_frozen_start_difficulty_row() {
        let hard = VmceHouseAssignments {
            difficulty: LobbyDifficulty::HARD.native_row(),
            ..Default::default()
        };
        let easy = VmceHouseAssignments {
            difficulty: LobbyDifficulty::EASY.native_row(),
            ..Default::default()
        };
        assert_eq!(result_difficulty(Some(&hard)), 2);
        assert_eq!(result_difficulty(Some(&easy)), 0);
        assert_eq!(
            result_difficulty(None),
            LobbyDifficulty::NORMAL.native_row()
        );
        // A corrupted frozen payload stays inside TriggerType's three rows.
        let out_of_range = VmceHouseAssignments {
            difficulty: 9,
            ..Default::default()
        };
        assert_eq!(result_difficulty(Some(&out_of_range)), 2);
    }
}
