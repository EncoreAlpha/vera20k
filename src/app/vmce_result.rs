//! VMCE's settlement page: a won mission, a lost mission and the campaign's
//! last mission all use retail's campaign score dialog `0x103` for rendering
//! and input (the dialog VMCE's victory already used), while this module owns
//! the result state, the network round and the outcome semantics. The stock
//! Cooperative mode keeps its own BC6 dialog in `cooperative_round`.
//!
//! The `0x103` Continue is the page's only action:
//! - a won mission with a next stage is the all-ready/start gate: a guest's
//!   Continue marks it ready, the host's starts the next stage;
//! - a defeat or the last mission acknowledges the page and leaves for the
//!   lobby on every peer;
//! - a solo host has no guest row to wait for and starts/leaves directly.

use crate::app::match_bootstrap::VmceNextStage;
use crate::app::{App, AppState};

/// Open the result page for a won mission and enter its round. The round is
/// entered even when this peer resolved no next stage: both peers exchange
/// their resolved stage, and a mismatch returns both to the lobby instead of
/// one peer waiting forever.
pub(crate) fn open_victory(state: &mut AppState) {
    let elapsed = state
        .match_state
        .scenario_elapsed_clock
        .stop_buckets(crate::app::match_runtime::frame_pacer::wall_clock_ms());
    let next = resolve_next_stage(state);
    let Some(rows) = score_rows(state) else {
        App::leave_mission_result_screen(state);
        return;
    };
    let model = crate::app::match_runtime::sim_tick::build_vmce_victory_score_model(state, elapsed);
    let can_continue = next.is_some();
    open_page(state, model, next, can_continue, rows, true, "victory");
}

/// Open the result page for a lost mission. A defeat retries the same
/// mission, so it never carries a next stage.
pub(crate) fn open_defeat(state: &mut AppState) {
    let elapsed = state
        .match_state
        .scenario_elapsed_clock
        .stop_buckets(crate::app::match_runtime::frame_pacer::wall_clock_ms());
    let Some(rows) = score_rows(state) else {
        App::leave_mission_result_screen(state);
        return;
    };
    let model = crate::app::match_runtime::sim_tick::build_vmce_victory_score_model(state, elapsed);
    open_page(state, model, None, false, rows, false, "defeat");
}

/// Whether the open `0x103` page is VMCE's. Stock modes keep their own
/// Continue handling.
pub(crate) fn owns_page(state: &AppState) -> bool {
    state.frontend.campaign_score_page.is_some()
        && (state
            .match_state
            .session
            .network_setup()
            .is_some_and(|launch| launch.mode.class.is_vmce())
            || state
                .match_state
                .session
                .cooperative_round()
                .is_some_and(|round| round.vera()))
}

/// Run the VMCE result page's Continue. Returns false when the open page is
/// not VMCE's, so the caller keeps the retail path.
pub(crate) fn continue_pressed(state: &mut AppState) -> bool {
    if !owns_page(state) {
        return false;
    }
    if state.match_state.session.cooperative_round().is_none() {
        // A result page always has its round; recover to the lobby rather
        // than leaving the page without a network owner.
        state.frontend.campaign_score_page = None;
        crate::app::lan_service::cooperative::release_transport(state);
        App::leave_mission_result_screen(state);
        return true;
    }
    if !state
        .match_state
        .session
        .cooperative_round_continue_enabled()
    {
        state.platform.window.request_redraw();
        return true;
    }
    let difficulty = state
        .frontend
        .multiplayer_shell_runtime
        .cooperative_difficulty();
    match state
        .match_state
        .session
        .continue_cooperative_round(difficulty)
    {
        Ok(step) => crate::app::cooperative_round::run_step(state, step),
        Err(error) => log::warn!("VMCE result Continue refused: {error:#}"),
    }
    state.platform.window.request_redraw();
    true
}

fn resolve_next_stage(state: &AppState) -> Option<VmceNextStage> {
    state
        .match_state
        .session
        .network_vmce_map_config()
        .cloned()
        .and_then(
            |current| match crate::app::shell_network_setup::vmce_map::resolve_next_vmce_stage(
                state, &current,
            ) {
                Ok(next) => next,
                Err(error) => {
                    log::warn!("VMCE next campaign stage is unavailable: {error:#}");
                    None
                }
            },
        )
}

fn score_rows(state: &AppState) -> Option<usize> {
    state
        .match_state
        .sim_runtime
        .as_ref()
        .map(|runtime| runtime.simulation.cooperative_score_rows().len())
}

fn open_page(
    state: &mut AppState,
    model: crate::ui::campaign_score_shell::CampaignScoreScreenModel,
    next: Option<VmceNextStage>,
    can_continue: bool,
    rows: usize,
    admit_slots: bool,
    what: &str,
) {
    state.frontend.score_page = None;
    state.frontend.score_art = None;
    state.frontend.coop_score_page = None;
    crate::app::lan_service::cooperative::hold_transport(state);
    App::retire_finished_scenario(state);
    if let Err(error) = state
        .match_state
        .session
        .enter_vmce_round(next, can_continue, rows)
    {
        log::error!("Could not open the VMCE {what} result: {error:#}");
        crate::app::cooperative_round::return_to_shell(state);
        return;
    }
    if admit_slots {
        // The repaint's country pass 0x0046F6D0 against the next stage.
        let runtime = &state.frontend.multiplayer_shell_runtime;
        let admit = |slot: &mut crate::skirmish_launch::SkirmishLocalSlot| {
            let requested = if slot.country_random {
                -2
            } else {
                i32::from(slot.country.raw_index())
            };
            if runtime.cooperative_admits(requested) {
                return;
            }
            match runtime.cooperative_default_country() {
                Some(country) => {
                    slot.country = country;
                    slot.country_random = false;
                }
                None => slot.country_random = true,
            }
        };
        if let Err(error) = state
            .match_state
            .session
            .admit_cooperative_round_countries(admit)
        {
            log::warn!("VMCE {what} round countries: {error:#}");
        }
    }
    App::open_campaign_score_page(state, model, String::new(), String::new());
}
