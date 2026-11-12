//! VMCE's `0xBC6` defeat/retry result page content.
//!
//! `0xBC6` is the native Cooperative result dialog; VERA's VMCE defeat page is
//! the statistics view itself: four authored role rows, no chat, no player
//! setup columns, no next mission, and Retry/Leave Room in place of the native
//! Continue/Quit. The shared `CoopScorePage` keeps owning the dialog and the
//! chat/score interaction state; this module owns the VMCE content and the
//! projection that builds the stock model's generic rows.
//!
//! Defeat statistics read the live, unbonused House values while the finished
//! world is still resident (never the terminal snapshot).

use crate::app::{App, AppState};

use super::types::VmceStatistics;
use super::victory::{VmceScoreSource, statistics_page};

/// The VMCE content over one open `0xBC6` page.
#[derive(Debug, Clone)]
pub(crate) struct DefeatResultPage {
    /// The statistics projection read before the finished world retired.
    pub(crate) statistics: VmceStatistics,
}

/// The open page's defeat content, if the installed extension is the defeat
/// page.
pub(crate) fn page(state: &AppState) -> Option<&DefeatResultPage> {
    match state.frontend.vmce_result_page.as_ref()? {
        super::VmceResultPage::Defeat(page) => Some(page),
        super::VmceResultPage::Victory(_) => None,
    }
}

/// Open the defeat page on the native BC6 shell (`0xBC6`), not `0x103`. Its
/// Continue is the retry vote/start gate; its Quit is Leave Room.
pub(super) fn open_defeat(state: &mut AppState) {
    let elapsed = state
        .match_state
        .scenario_elapsed_clock
        .stop_buckets(crate::app::match_runtime::frame_pacer::wall_clock_ms());
    let Some(model) = build_defeat_model(state, elapsed) else {
        App::leave_mission_result_screen(state);
        return;
    };
    // The failure page shows the same statistics table as the victory stats
    // page: score rows plus the gross-income accumulator and the stage's par
    // time. Project it while the finished world is still resident.
    let ranking_model =
        crate::app::match_runtime::sim_tick::build_vmce_victory_score_model(state, elapsed);
    let Some(statistics) = statistics_page(state, &ranking_model, VmceScoreSource::Live) else {
        log::error!("Could not build the VMCE defeat statistics");
        App::leave_mission_result_screen(state);
        return;
    };
    let rows = model.rows.len();
    crate::app::lan_service::cooperative::hold_transport(state);
    App::retire_finished_scenario(state);
    if let Err(error) = state.match_state.session.enter_vmce_round_ending(
        None,
        true,
        crate::app::match_runtime::session::VmceRoundEnding::Retry,
        rows,
    ) {
        log::error!("Could not open the VMCE defeat round: {error:#}");
        crate::app::cooperative_round::return_to_shell(state);
        return;
    }
    install_defeat_page(state, model, statistics);
}

/// Present an already-built VMCE defeat page on the native BC6 shell. Both the
/// real defeat path and the synthetic captures use this one page owner.
pub(crate) fn install_defeat_page(
    state: &mut AppState,
    model: crate::ui::coop_score_shell::CoopScoreModel,
    statistics: VmceStatistics,
) {
    let side = model.side;
    // The shared BC6 backdrop is the native campaign score SCRBK.
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
    state.frontend.coop_score_page = Some(crate::ui::coop_score_shell::CoopScorePage {
        model,
        ui: Default::default(),
        victory: false,
        saved: None,
        campaign_end: None,
    });
    state.frontend.vmce_result_page = Some(super::VmceResultPage::Defeat(DefeatResultPage {
        statistics,
    }));
    state.frontend.screen = crate::app::GameScreen::MissionResult {
        title: String::new(),
        detail: String::new(),
    };
    state
        .frontend
        .shell_controller
        .reset_to(crate::ui::coop_score_shell::COOP_SCORE_DIALOG, false);
}

/// The shared BC6 model over the four authored role rows in campaign order
/// (never sorted), no next mission/preview. The VMCE mode label is its own
/// display name: the external campaign has no CSF entry for that row. The row,
/// time, side and map projections come from the shared co-op result owner.
pub(crate) fn build_defeat_model(
    state: &AppState,
    elapsed: i32,
) -> Option<crate::ui::coop_score_shell::CoopScoreModel> {
    use crate::app::frontend::shell_pass::resolve_csf;
    let rows = crate::app::cooperative_round::score_rows(state)?;
    let (_, map_name) = crate::app::cooperative_round::score_map(state);
    let mode_label = crate::app::cooperative_round::score_mode_key(state)
        .map_or_else(|| resolve_csf(state, "GUI:None").into_owned(), |key| key);
    Some(crate::ui::coop_score_shell::CoopScoreModel {
        rows,
        time: crate::app::cooperative_round::score_time(state, elapsed),
        next_mission: String::new(),
        mode_label,
        map_label: map_name,
        description: String::new(),
        side: crate::app::cooperative_round::score_side(state),
        map_index: None,
        can_continue: true,
    })
}

#[cfg(test)]
mod tests {
    use crate::app::vmce_result::types::{DEFEAT_SCORE_ROWS, VMCE_ROW_CELLS};
    use crate::ui::coop_score_shell::{CONTINUE, CoopScoreLayout, DEFAULT_SCORE_ROWS, ROW_CELLS};

    #[test]
    fn the_defeat_page_extends_bc6_to_four_native_rows() {
        let stock = CoopScoreLayout::new(800, 600, false);
        let four = CoopScoreLayout::new_rows(800, 600, false, DEFEAT_SCORE_ROWS, &VMCE_ROW_CELLS);
        assert_eq!(four.rows(), DEFEAT_SCORE_ROWS);
        assert_eq!(stock.rows(), DEFAULT_SCORE_ROWS);
        // The native two rows keep their executed geometry.
        for row in 0..2 {
            for cell in 0..5 {
                let id = stock.row_cells(row).unwrap()[cell];
                assert_eq!(four.control_rect(id), stock.control_rect(id));
            }
        }
        // The extended rows reuse the native column x/width on the native
        // 14-DLU pitch.
        for row in [2, 3] {
            let cells = four.row_cells(row).expect("extended row cells");
            for (column, id) in cells.iter().enumerate() {
                let native = four
                    .control_rect(ROW_CELLS[0][column])
                    .expect("native column");
                let extended = four.control_rect(*id).expect("extended cell rect");
                assert_eq!(
                    (extended.x, extended.w),
                    (native.x, native.w),
                    "row {row} column {column}"
                );
            }
            let previous = four
                .control_rect(four.row_cells(row - 1).unwrap()[0])
                .expect("previous row");
            let current = four.control_rect(cells[0]).expect("extended row");
            let pitch = (current.y - previous.y).abs();
            assert!((pitch - 23).abs() <= 1, "native 14-DLU pitch: {pitch}");
        }
        // The four rows clear the lower decorative area, the status line and
        // the Continue button.
        let last = four.control_rect(four.row_cells(3).unwrap()[0]).unwrap();
        assert!(last.y + last.h < four.page.status_help.y);
        let continue_button = four.page.button_rect(CONTINUE).expect("Continue");
        assert!(last.y + last.h < continue_button.y);
        assert!(last.y + last.h < 400);
    }
}
