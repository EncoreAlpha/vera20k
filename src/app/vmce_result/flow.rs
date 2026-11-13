//! VMCE's result-page projections: score rows, stage labels and the native
//! campaign score model. These read the shared simulation and VERA's base UI
//! model; no Cooperative result owner is involved.

use crate::app::AppState;

/// The campaign score model for a VMCE victory/defeat page.
pub(crate) fn build_vmce_victory_score_model(
    state: &AppState,
    elapsed_score_ticks: i32,
) -> crate::ui::campaign_score_shell::CampaignScoreScreenModel {
    let difficulty = state
        .frontend
        .multiplayer_shell_runtime
        .cooperative_difficulty();
    let houses = state.match_state.sim_runtime.runtime().map(|runtime| {
        // Native 0x6C92C1 sums PlayerControl Houses; a VMCE role's primary
        // House is player-controlled through the campaign grant table instead,
        // so read the role rows the mission outcome already uses. A shared or
        // observer House is not a row and must not add its kills.
        let kills = runtime
            .simulation
            .cooperative_score_rows()
            .iter()
            .fold(0u32, |sum, row| sum.wrapping_add(row.kills));
        vec![(true, kills)]
    });
    let side = state
        .match_state
        .sim_runtime
        .runtime()
        .and_then(|runtime| {
            crate::app::input::commands::preferred_local_owner_name(state)
                .as_deref()
                .and_then(|owner| runtime.simulation.interner().get(owner))
                .and_then(|owner| runtime.simulation.houses().get(&owner))
                .map(|house| house.side_index())
        })
        .unwrap_or(0);
    let mut model = build_campaign_score_screen_model_from_sources(
        elapsed_score_ticks,
        difficulty,
        &state.match_state.map_ranking,
        houses.unwrap_or_default(),
        side,
        |key| resolve_campaign_score_label(state, key),
    );
    model.theme = state
        .process_assets
        .side_score_theme(
            side,
            crate::rules::ruleset::ScoreTheme::Campaign {
                over_par: model.over_par,
            },
        )
        .map(str::to_owned);
    model
}

fn build_campaign_score_screen_model_from_sources(
    elapsed_score_ticks: i32,
    difficulty: i32,
    ranking: &crate::map::ranking::RankingSection,
    houses: impl IntoIterator<Item = (bool, u32)>,
    side: i32,
    mut resolve_label: impl FnMut(&str) -> String,
) -> crate::ui::campaign_score_shell::CampaignScoreScreenModel {
    let mut model = crate::ui::campaign_score_shell::CampaignScoreScreenModel {
        side,
        ..Default::default()
    };
    let elapsed_score_seconds = elapsed_score_ticks / 60;
    model.your_time = format_campaign_score_time(elapsed_score_seconds);
    let par_score_ticks = ranking.par_time_for_difficulty(difficulty);
    let par_score_seconds = par_score_ticks / 60;
    model.par_time = format_campaign_score_time(par_score_seconds);
    model.over_par = elapsed_score_seconds > par_score_seconds;
    let (rank_key, commendation_key) = if elapsed_score_seconds <= par_score_seconds {
        (&ranking.under_par_title, &ranking.under_par_message)
    } else {
        (&ranking.over_par_title, &ranking.over_par_message)
    };
    model.rank = resolve_label(rank_key);
    model.commendation = resolve_label(commendation_key);
    let enemies_destroyed = houses
        .into_iter()
        .fold(0u32, |sum, (player_control, kills)| {
            if player_control {
                sum.wrapping_add(kills)
            } else {
                sum
            }
        });
    model.enemies_destroyed = enemies_destroyed.to_string();
    model
}

fn resolve_campaign_score_label(state: &AppState, native_key: &str) -> String {
    crate::app::frontend::shell_pass::resolve_csf(state, native_key).into_owned()
}

fn format_campaign_score_time(total_seconds: i32) -> String {
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
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
