//! VMCE's settlement pages and round semantics.
//!
//! Victory and campaign end use the campaign score dialog `0x103`; defeat uses
//! the cooperative BC6 dialog. This module owns every VMCE-specific result
//! fact — the settlement board, the statistics projection and subpage state,
//! and the right-panel button projection — while the shared UI models keep
//! only the native dialogs. Guests ready on victory and vote to retry on
//! defeat; the host continues once every connected guest is ready. Campaign
//! end uses the all-ready gate.
//!
//! The session/round/LAN owners (B2) still decide preparation, retry,
//! continuation, leaving and campaign end; a result page only projects their
//! state and hands button intents back to them.

pub(crate) mod defeat;
pub(crate) mod paint_defeat;
pub(crate) mod paint_victory;
pub(crate) mod types;
pub(crate) mod victory;

use crate::app::AppState;

use types::{VMCE_LEAVE_CONTROL, VMCE_READY_CONTROL, VMCE_STATS_BACK_CONTROL, VMCE_STATS_CONTROL};
use victory::VictoryResultPage;

/// VERA's result-page extension over one stock result dialog. Both the VMCE
/// content and its stock page are installed and dropped together; the stock
/// page owns the native dialog, this container owns what VERA adds.
#[derive(Debug, Clone)]
pub(crate) enum VmceResultPage {
    /// `0x103` victory or campaign end (stock page: `campaign_score_page`).
    Victory(VictoryResultPage),
    /// `0xBC6` defeat/retry (stock page: `coop_score_page`).
    Defeat(defeat::DefeatResultPage),
}

impl VmceResultPage {
    /// The settlement board of a victory/campaign-end page.
    pub(crate) fn victory(&self) -> Option<&VictoryResultPage> {
        match self {
            Self::Victory(page) => Some(page),
            Self::Defeat(_) => None,
        }
    }

    /// The statistics content of the open page.
    pub(crate) fn statistics(&self) -> Option<&types::VmceStatistics> {
        match self {
            Self::Victory(page) => page.statistics.as_ref(),
            Self::Defeat(page) => Some(&page.statistics),
        }
    }
}

/// Decide whether the finished match settles on a VMCE page; when it does,
/// open the page and return true so the caller keeps its stock path only for
/// ordinary Cooperative. This is the only mode check for the result path.
pub(crate) fn open_result(state: &mut AppState) -> bool {
    let settlement = {
        let Some(sim) = state
            .match_state
            .sim_runtime
            .as_ref()
            .map(|runtime| &runtime.simulation)
            .filter(|sim| {
                sim.session()
                    .multiplayer_mode_class()
                    .is_some_and(|class| class.is_vmce())
            })
        else {
            return false;
        };
        sim.session().vmce_outcomes().map(|outcomes| {
            outcomes
                .iter()
                .any(|(_, kind)| *kind == crate::sim::house_state::HouseOutcomeKind::Victory)
        })
    };
    match settlement {
        None => {
            log::error!("VMCE reached settlement without a shared result");
            crate::app::cooperative_round::return_to_shell(state);
        }
        Some(true) => victory::open_victory(state),
        Some(false) => defeat::open_defeat(state),
    }
    true
}

/// Whether the open result page carries VMCE content.
pub(crate) fn owns_page(state: &AppState) -> bool {
    state.frontend.vmce_result_page.is_some()
}

/// Whether the open BC6 page is VERA's statistics-only defeat page.
pub(crate) fn defeat_page_active(state: &AppState) -> bool {
    matches!(
        state.frontend.vmce_result_page,
        Some(VmceResultPage::Defeat(_))
    )
}

/// The BC6 row count and extra cell ids VERA's defeat page needs.
pub(crate) fn row_extension(state: &AppState) -> Option<(usize, &'static [[u16; 5]; 2])> {
    defeat_page_active(state).then_some((types::DEFEAT_SCORE_ROWS, &types::VMCE_ROW_CELLS))
}

/// Drop the VMCE content with the stock result page it decorates. Every
/// result-page teardown goes through this owner.
pub(crate) fn close_vmce_result(state: &mut AppState) {
    state.frontend.vmce_result_page = None;
}

/// Install the VMCE content over an already-open stock `0x103` page. The real
/// victory/campaign-end flow and the diagnostics fixtures share this one
/// install; the stock page itself stays the production `App` page.
pub(crate) fn install_victory_content(
    state: &mut AppState,
    board: Option<types::VmceResultBoard>,
    statistics: Option<types::VmceStatistics>,
    stats_open: bool,
) {
    let mut page = VictoryResultPage::new(board, statistics);
    if stats_open {
        page.toggle_statistics();
    }
    state.frontend.vmce_result_page = Some(VmceResultPage::Victory(page));
}

/// The button a released VMCE result-page control asks for. Kept as data so
/// paint and input consume the same page state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonIntent {
    ToggleStatistics,
    CloseStatistics,
    LeaveRoom,
    Ready,
    Continue,
}

/// Map one released controller id on the open 0x103 page. `continue_id` is the
/// native dialog's result control.
pub(crate) fn victory_button_intent(
    page: &VictoryResultPage,
    id: u16,
    continue_id: u16,
) -> Option<ButtonIntent> {
    if id == continue_id {
        // The native result control stays routable even when the board could
        // not be projected: a VMCE 0x103 page always reacts to its Continue.
        return Some(ButtonIntent::Continue);
    }
    let board = page.board.as_ref()?;
    match id {
        VMCE_STATS_CONTROL if board.victory && page.statistics.is_some() => {
            Some(ButtonIntent::ToggleStatistics)
        }
        VMCE_STATS_BACK_CONTROL if board.victory => Some(ButtonIntent::CloseStatistics),
        VMCE_LEAVE_CONTROL => Some(ButtonIntent::LeaveRoom),
        VMCE_READY_CONTROL if !board.local_host && !board.local_ready => Some(ButtonIntent::Ready),
        _ => None,
    }
}

/// Run a released VMCE result-page control. Returns false when the page is not
/// VMCE's or the control is not its button, so the caller keeps the retail
/// path.
pub(crate) fn button_pressed(state: &mut AppState, released: Option<u16>) -> bool {
    let Some(id) = released else {
        return false;
    };
    let intent = {
        let Some(page) = state.frontend.vmce_result_page.as_ref() else {
            return false;
        };
        match page {
            VmceResultPage::Victory(victory_page) => {
                let continue_id = state
                    .frontend
                    .campaign_score_page
                    .as_ref()
                    .map_or(0, |page| page.dialog.continue_button());
                victory_button_intent(victory_page, id, continue_id)
            }
            // The BC6 defeat page's Continue/Quit route through the B2 round
            // owner's `CoopScoreAction`; its statistics page has no buttons.
            VmceResultPage::Defeat(_) => None,
        }
    };
    match intent {
        None => false,
        Some(ButtonIntent::ToggleStatistics) => {
            if let Some(VmceResultPage::Victory(page)) = state.frontend.vmce_result_page.as_mut() {
                page.toggle_statistics();
            }
            state.platform.window.request_redraw();
            true
        }
        Some(ButtonIntent::CloseStatistics) => {
            if let Some(VmceResultPage::Victory(page)) = state.frontend.vmce_result_page.as_mut() {
                page.close_statistics();
            }
            state.platform.window.request_redraw();
            true
        }
        Some(ButtonIntent::LeaveRoom) => leave_room_pressed(state),
        Some(ButtonIntent::Ready | ButtonIntent::Continue) => victory::continue_pressed(state),
    }
}

/// Run a VMCE result page's Leave Room: the round's own leave record goes to
/// every peer (`0x0046E28C`'s `0x24`), the page closes and the LAN lobby
/// returns. The host's leave disbands the room; a guest's leave is its local
/// exit and the host's own peer-left path returns the room to the lobby.
pub(crate) fn leave_room_pressed(state: &mut AppState) -> bool {
    if !owns_page(state) {
        return false;
    }
    if let Err(error) = state.match_state.session.quit_cooperative_round() {
        log::warn!("VMCE result Leave Room refused: {error:#}");
    }
    crate::app::cooperative_round::return_to_shell(state);
    true
}

/// Refresh the open victory page after the round owner changes Ready or
/// membership. A defeat page reads the round live in its painter.
pub(crate) fn refresh(state: &mut AppState) {
    if !matches!(
        state.frontend.vmce_result_page,
        Some(VmceResultPage::Victory(_))
    ) {
        return;
    }
    let board = victory::result_board(state);
    if let Some(VmceResultPage::Victory(page)) = state.frontend.vmce_result_page.as_mut() {
        page.refresh(board);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{VmceResultBoard, VmceStatistics};

    fn stats() -> VmceStatistics {
        VmceStatistics {
            rows: Vec::new(),
            total_time: String::new(),
            par_time: String::new(),
        }
    }

    fn victory_page(host: bool, ready: bool, can_continue: bool) -> VictoryResultPage {
        let board = VmceResultBoard {
            rows: Vec::new(),
            victory: true,
            local_host: host,
            local_ready: ready,
            can_continue,
        };
        let mut page = VictoryResultPage::new(Some(board), Some(stats()));
        page.toggle_statistics();
        page
    }

    #[test]
    fn the_victory_buttons_map_to_the_one_projection() {
        let page = victory_page(true, false, false);
        assert_eq!(
            victory_button_intent(&page, VMCE_STATS_CONTROL, 0x423),
            Some(ButtonIntent::ToggleStatistics)
        );
        assert_eq!(
            victory_button_intent(&page, VMCE_LEAVE_CONTROL, 0x423),
            Some(ButtonIntent::LeaveRoom)
        );
        assert_eq!(
            victory_button_intent(&page, 0x423, 0x423),
            Some(ButtonIntent::Continue),
            "the host's native Continue stays reachable"
        );
        let guest = victory_page(false, false, false);
        assert_eq!(
            victory_button_intent(&guest, VMCE_READY_CONTROL, 0x423),
            Some(ButtonIntent::Ready)
        );
    }

    #[test]
    fn an_already_voted_ready_button_is_not_an_intent() {
        let page = victory_page(false, true, false);
        assert_eq!(
            victory_button_intent(&page, VMCE_READY_CONTROL, 0x423),
            None,
            "a cast vote keeps the lit frame but takes no press"
        );
        assert_eq!(
            victory_button_intent(&page, VMCE_STATS_BACK_CONTROL, 0x423),
            Some(ButtonIntent::CloseStatistics)
        );
    }

    #[test]
    fn the_statistics_back_control_only_exists_on_the_stats_page() {
        let mut page = victory_page(true, false, true);
        page.close_statistics();
        // Back is still mapped (the controller only feeds it on the stats
        // page); the toggle remains available on the board.
        assert_eq!(
            victory_button_intent(&page, VMCE_STATS_BACK_CONTROL, 0x423),
            Some(ButtonIntent::CloseStatistics)
        );
        assert_eq!(
            victory_button_intent(&page, VMCE_STATS_CONTROL, 0x423),
            Some(ButtonIntent::ToggleStatistics)
        );
    }

    #[test]
    fn a_non_victory_board_has_only_the_native_continue() {
        let board = VmceResultBoard {
            rows: Vec::new(),
            victory: false,
            local_host: true,
            local_ready: false,
            can_continue: true,
        };
        let page = VictoryResultPage::new(Some(board), None);
        assert_eq!(
            victory_button_intent(&page, VMCE_STATS_CONTROL, 0x423),
            None
        );
        assert_eq!(
            victory_button_intent(&page, 0x423, 0x423),
            Some(ButtonIntent::Continue)
        );
    }
}
