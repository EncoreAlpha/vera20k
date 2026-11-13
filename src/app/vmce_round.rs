//! VMCE's result-round app owner: assignment, retry, next mission, campaign
//! end and the return to the shell. Nothing here calls the stock Cooperative
//! result flow.

use crate::app::match_runtime::session::{VmceResultEvent, VmceRoundEnding, VmceRoundStep};
use crate::app::{App, AppState};

/// Whether the terminal mission outcome belongs to a VMCE result round.
pub(crate) fn applies(state: &AppState) -> bool {
    state.match_state.session_mode().is_network()
        && state
            .match_state
            .sim_runtime
            .runtime()
            .and_then(|runtime| runtime.simulation.session().multiplayer_mode_class())
            .is_some_and(|class| class.is_vmce())
}

pub(crate) fn service_event(state: &mut AppState, event: VmceResultEvent) -> anyhow::Result<()> {
    let step = state.match_state.session.vmce_result_event(event)?;
    run_step(state, step);
    // Ready/Leave events change rows the open settlement page shows; the
    // page reads the round, so refresh it before the next frame paints.
    crate::app::vmce_result::refresh(state);
    Ok(())
}

pub(crate) fn run_step(state: &mut AppState, step: VmceRoundStep) {
    match step {
        VmceRoundStep::None | VmceRoundStep::ContinueInRoom => {}
        VmceRoundStep::Exit(_) => return_to_shell(state),
        VmceRoundStep::Assign { ending } => {
            if let Err(error) = assign(state, ending) {
                log::error!("VMCE result assignment failed: {error:#}");
                let _ = state.match_state.session.quit_vmce_result();
                return_to_shell(state);
            }
        }
        VmceRoundStep::Next => {
            if state.match_state.session_mode()
                == crate::app::match_runtime::session::SessionMode::Lan
            {
                unlock_confirmed_stage(state);
                advance_intermission(state);
            } else {
                begin_next_mission(state);
            }
        }
        VmceRoundStep::Retry => retry_last_mission(state),
        VmceRoundStep::CampaignEnd => {
            if crate::app::lan_service::vmce_round::defer_campaign_completion(state) {
                return;
            }
            App::vmce_campaign_end(state);
        }
    }
}

fn assign(state: &mut AppState, ending: VmceRoundEnding) -> anyhow::Result<()> {
    if ending == VmceRoundEnding::Next
        && state.match_state.session_mode() == crate::app::match_runtime::session::SessionMode::Lan
    {
        crate::app::lan_service::vmce_round::begin_vmce_intermission_transition(state)?;
        let now = crate::app::match_runtime::frame_pacer::frame_bucket(
            crate::app::match_runtime::frame_pacer::wall_clock_ms(),
        );
        if !crate::app::lan_service::vmce_round::vmce_intermission_transition_ready(state, now)? {
            return Ok(());
        }
    }
    if state.match_state.session_mode() == crate::app::match_runtime::session::SessionMode::Lan {
        crate::app::lan_service::vmce_round::mark_assignment_wait(state);
    }
    state.match_state.session.vmce_result_assigned()?;
    if state.match_state.session_mode() == crate::app::match_runtime::session::SessionMode::Lan
        && state.match_state.session.host()
    {
        state.match_state.session.vmce_result_queue_go()?;
    }
    match ending {
        VmceRoundEnding::Next => {
            if state.match_state.session_mode()
                == crate::app::match_runtime::session::SessionMode::Lan
            {
                unlock_confirmed_stage(state);
                advance_intermission(state);
            } else {
                begin_next_mission(state);
            }
        }
        VmceRoundEnding::Retry => {
            if state.match_state.session_mode()
                == crate::app::match_runtime::session::SessionMode::Lan
                && state.match_state.session.host()
            {
                // The adapter waits for the Go's reliable delivery before the
                // host freezes into the reload; freezing now would drop it.
            } else {
                retry_last_mission(state);
            }
        }
        VmceRoundEnding::CampaignEnd => {
            if crate::app::lan_service::vmce_round::defer_campaign_completion(state) {
                return Ok(());
            }
            App::vmce_campaign_end(state);
        }
    }
    Ok(())
}

/// Reopen the resolved next stage after every peer agreed on the result.
pub(crate) fn advance_intermission(state: &mut AppState) {
    if state.match_state.session.host()
        && state
            .match_state
            .session
            .network_connected_source_indices()
            .len()
            == 1
        && let Err(error) =
            crate::app::lan_service::vmce_round::begin_vmce_intermission_transition(state)
    {
        log::error!("VMCE solo intermission preparation failed: {error:#}");
        return_to_shell(state);
        return;
    }
    match crate::app::lan_service::vmce_round::enter_vmce_intermission(state) {
        Ok(false) => {}
        Ok(true) => {
            state.frontend.campaign_score_page = None;
            state.frontend.campaign_score_art = None;
            crate::app::vmce_result::close_vmce_result(state);
            if let Err(error) = crate::app::shell_network_setup::open_vmce_intermission(state) {
                log::error!("Could not open VMCE intermission: {error:#}");
                return_to_shell(state);
            }
        }
        Err(error) => {
            log::error!("VMCE intermission handoff failed: {error:#}");
            return_to_shell(state);
            crate::app::lan_service::vmce_round::report_round_failure(state);
        }
    }
}

/// Result 1: reload the same frozen stage for a defeat retry; a win reloads the
/// resolved next stage in a fresh room instead.
pub(crate) fn retry_last_mission(state: &mut AppState) {
    state.frontend.campaign_score_page = None;
    state.frontend.coop_score_page = None;
    state.frontend.campaign_score_art = None;
    crate::app::vmce_result::close_vmce_result(state);
    let game_speed = state
        .match_state
        .session
        .network_setup()
        .map_or(0, |launch| launch.options.game_speed);
    let (max_ahead, latency_fudge) =
        crate::net::lockstep::cooperative_continuation_timing(game_speed, &[]);
    match state
        .match_state
        .session
        .freeze_vmce_result(max_ahead, latency_fudge, None)
    {
        Ok(startup) => crate::app::loading::pump::begin_loading(
            state,
            crate::app::loading::pump::LoadingRequest::network(startup),
        ),
        Err(error) => {
            log::error!("Could not retry the VMCE mission: {error:#}");
            return_to_shell(state);
        }
    }
}

/// Load the next campaign mission in the same match (non-LAN continuation).
fn begin_next_mission(state: &mut AppState) {
    state.frontend.campaign_score_page = None;
    crate::app::vmce_result::close_vmce_result(state);
    let game_speed = state
        .match_state
        .session
        .network_setup()
        .map_or(0, |launch| launch.options.game_speed);
    let (max_ahead, latency_fudge) =
        crate::net::lockstep::cooperative_continuation_timing(game_speed, &[]);
    state.frontend.campaign_score_art = None;
    match state
        .match_state
        .session
        .freeze_vmce_result(max_ahead, latency_fudge, None)
    {
        Ok(startup) => crate::app::loading::pump::begin_loading(
            state,
            crate::app::loading::pump::LoadingRequest::network(startup),
        ),
        Err(error) => {
            log::error!("Could not continue the VMCE campaign: {error:#}");
            return_to_shell(state);
        }
    }
}


/// Main_Game's return after GameExit: the mode's shell.
pub(crate) fn return_to_shell(state: &mut AppState) {
    if crate::app::lan_service::vmce_round::defer_vmce_round_exit(state) {
        return;
    }
    finish_return_to_shell(state);
}

pub(crate) fn finish_return_to_shell(state: &mut AppState) {
    crate::app::lan_service::vmce_round::release_round_transport(state);
    leave_result(state);
}

fn leave_result(state: &mut AppState) {
    state.frontend.coop_score_page = None;
    state.frontend.campaign_score_page = None;
    crate::app::vmce_result::close_vmce_result(state);
    state.frontend.campaign_score_art = None;
    App::persist_returned_session_toggles(state);
    state.frontend.screen = crate::app::GameScreen::MainMenu;
    App::resume_shell_after_match(state);
}

fn unlock_confirmed_stage(state: &AppState) {
    let Some(map) = state.match_state.session.vmce_round_next_stage_map() else {
        return;
    };
    // A completed mission unlocks its resolved successor even if loading it
    // later fails.
    crate::app::vmce_campaign::unlock_catalog_stage(state, map);
}
