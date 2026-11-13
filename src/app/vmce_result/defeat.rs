//! VMCE's defeat/retry result page: its own statistics content, layout and
//! Retry/Leave actions. The stock BC6 Cooperative page is not opened.

use crate::app::{App, AppState};

use super::statistics::{VmceScoreSource, statistics};
use super::types::VmceStatistics;

/// A released control of VMCE's defeat page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmceDefeatAction {
    Retry,
    Leave,
}

/// The VMCE defeat page content.
#[derive(Debug, Clone)]
pub(crate) struct DefeatResultPage {
    /// The statistics projection read before the finished world retired.
    pub(crate) statistics: VmceStatistics,
    /// The right-panel control held by the current pointer press.
    pub(crate) pressed: Option<u16>,
}

/// The open page's defeat content, if the installed extension is the defeat
/// page.
pub(crate) fn page(state: &AppState) -> Option<&DefeatResultPage> {
    match state.frontend.vmce_result_page.as_ref()? {
        super::VmceResultPage::Defeat(page) => Some(page),
        super::VmceResultPage::Victory(_) => None,
    }
}

/// Open the VMCE defeat page after a lost mission. Its content is the
/// statistics table plus Retry/Leave Room; Retry is the retry vote/start gate.
pub(super) fn open_defeat(state: &mut AppState) {
    let elapsed = state
        .match_state
        .scenario_elapsed_clock
        .stop_buckets(crate::app::match_runtime::frame_pacer::wall_clock_ms());
    let Some(role_rows) = super::statistics::role_rows(state) else {
        log::error!("Could not build the VMCE defeat statistics");
        App::leave_mission_result_screen(state);
        return;
    };
    let ranking_model = super::flow::build_vmce_score_model(state, elapsed, &role_rows);
    let statistics = statistics(state, &role_rows, &ranking_model, VmceScoreSource::Live);
    let rows = statistics.rows.len();
    crate::app::lan_service::vmce_round::hold_round_transport(state);
    App::retire_finished_scenario(state);
    if let Err(error) = state.match_state.session.enter_vmce_round_ending(
        None,
        true,
        crate::app::match_runtime::session::VmceRoundEnding::Retry,
        rows,
    ) {
        log::error!("Could not open the VMCE defeat round: {error:#}");
        crate::app::vmce_round::return_to_shell(state);
        return;
    }
    install_defeat_page(state, statistics);
}

/// Present an already-built VMCE defeat page. The real defeat path and the
/// synthetic captures share this owner.
pub(crate) fn install_defeat_page(state: &mut AppState, statistics: VmceStatistics) {
    let side = super::flow::score_side(state);
    // The native campaign score SCRBK is the page backdrop.
    state.frontend.campaign_score_art = state.process_assets.manager().and_then(|assets| {
        crate::render::main_menu_shell_chrome::build_campaign_score_art(
            &state.renderer.gpu,
            &state.renderer.batch_renderer,
            assets,
            side,
            crate::app::frontend::shell_layout::art_key_screen_width(state),
            state.process_assets.side_score_art(side),
        )
    });
    state.frontend.score_page = None;
    state.frontend.score_art = None;
    state.frontend.campaign_score_page = None;
    state.frontend.coop_score_page = None;
    state.frontend.vmce_result_page = Some(super::VmceResultPage::Defeat(DefeatResultPage {
        statistics,
        pressed: None,
    }));
    state.frontend.screen = crate::app::GameScreen::MissionResult {
        title: String::new(),
        detail: String::new(),
    };
}

/// Route a control on the VMCE defeat page to VMCE's round.
pub(crate) fn apply_action(state: &mut AppState, action: VmceDefeatAction) {
    match action {
        VmceDefeatAction::Retry => {
            let result = if state.match_state.session.host() {
                state.match_state.session.vmce_result_retry_start()
            } else {
                state.match_state.session.vmce_result_retry_vote()
            };
            match result {
                Ok(step) => crate::app::vmce_round::run_step(state, step),
                Err(error) => log::warn!("VMCE retry refused: {error:#}"),
            }
        }
        VmceDefeatAction::Leave => {
            super::leave_room_pressed(state);
        }
    }
    state.platform.window.request_redraw();
}

/// The page's Retry projection: `(host, lit, enabled)`.
pub(crate) fn retry_state(state: &AppState) -> (bool, bool, bool) {
    state
        .match_state
        .session
        .vmce_result_retry_state()
        .unwrap_or((false, false, false))
}

/// Leave Room is always available while the page is open.
pub(crate) fn leave_enabled(state: &AppState) -> bool {
    page(state).is_some() && state.match_state.session.vmce_result_phase().is_some()
}
