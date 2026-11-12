//! VMCE's `0x103` victory and campaign-end result page content.
//!
//! The stock `CampaignScorePage` owns the native dialog, its statics and its
//! reveal/emblem timing; this module owns everything VERA adds on top: the
//! settlement board, the statistics projection and the statistics subpage
//! state. It installs that content together with the stock page through
//! [`super::VmceResultPage`] and never touches the shared UI model.
//!
//! Victory statistics prefer the terminal snapshot; campaign end reads the
//! same terminal snapshot, while the defeat page (see [`super::defeat`]) reads
//! the live values before the world retires.

use crate::app::match_bootstrap::VmceNextStage;
use crate::app::{App, AppState};

use super::types::{VmceResultBoard, VmceResultRow, VmceStatistics, VmceStatisticsRow};

/// The VMCE content over one open `0x103` page.
#[derive(Debug, Clone)]
pub(crate) struct VictoryResultPage {
    /// The settlement board projected at open time and refreshed as the round
    /// owner changes readiness or membership.
    pub(crate) board: Option<VmceResultBoard>,
    /// Statistics projected before the finished world retired.
    pub(crate) statistics: Option<VmceStatistics>,
    /// Whether the full-screen statistics subpage is showing.
    pub(crate) stats_open: bool,
}

impl VictoryResultPage {
    pub(crate) fn new(board: Option<VmceResultBoard>, statistics: Option<VmceStatistics>) -> Self {
        Self {
            board,
            statistics,
            stats_open: false,
        }
    }

    /// The page keeps the statistics entry only while there is content.
    pub(crate) fn toggle_statistics(&mut self) {
        if self.statistics.is_some() {
            self.stats_open = !self.stats_open;
        }
    }

    pub(crate) fn close_statistics(&mut self) {
        self.stats_open = false;
    }

    pub(crate) fn statistics_open(&self) -> bool {
        self.stats_open && self.statistics.is_some()
    }

    /// Refresh the board after the round owner changed Ready or membership.
    pub(crate) fn refresh(&mut self, board: Option<VmceResultBoard>) {
        self.board = board;
    }
}

/// Open the result page for a won mission and enter its round. The round is
/// entered even when this peer resolved no next stage: both peers exchange
/// their resolved stage, and a mismatch returns both to the lobby instead of
/// one peer waiting forever.
pub(super) fn open_victory(state: &mut AppState) {
    let elapsed = state
        .match_state
        .scenario_elapsed_clock
        .stop_buckets(crate::app::match_runtime::frame_pacer::wall_clock_ms());
    let next = match resolve_next_stage(state) {
        Ok(outcome) => outcome,
        Err(error) => {
            // A broken `campaign.ini` or a map that disappeared is not the
            // campaign's last stage: do not show a false end page, send both
            // peers back to the lobby through the round's leave path.
            log::error!("VMCE next campaign stage is unavailable: {error:#}");
            leave_after_stage_error(state);
            return;
        }
    };
    let Some(rows) = score_rows(state) else {
        App::leave_mission_result_screen(state);
        return;
    };
    let model = crate::app::match_runtime::sim_tick::build_vmce_victory_score_model(state, elapsed);
    let (next, ending) = match next {
        crate::app::shell_network_setup::vmce_map::VmceResolvedStage::Next(next) => (
            Some(next),
            crate::app::match_runtime::session::VmceRoundEnding::Next,
        ),
        crate::app::shell_network_setup::vmce_map::VmceResolvedStage::EndOfGame => (
            None,
            crate::app::match_runtime::session::VmceRoundEnding::CampaignEnd,
        ),
    };
    open_page(
        state, model, next, true, ending, rows, true, "victory", true,
    );
}

/// Run a VMCE victory/campaign-end page's Continue. Returns false when the
/// open page is not VMCE's, so the caller keeps the retail path.
pub(super) fn continue_pressed(state: &mut AppState) -> bool {
    if !super::owns_page(state) {
        return false;
    }
    if state.match_state.session.cooperative_round().is_none() {
        // A result page always has its round; recover to the lobby rather
        // than leaving the page without a network owner.
        super::close_vmce_result(state);
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
    super::refresh(state);
    state.platform.window.request_redraw();
    true
}

/// The settlement board the `0x103` page shows: one row per connected human
/// with its authored role, country and verdict, plus the truthful continuation
/// state for the ending this page gates. Read-only projection of the round and
/// launch the page already owns; no second result state.
pub(crate) fn result_board(state: &AppState) -> Option<VmceResultBoard> {
    let round = state.match_state.session.cooperative_round()?;
    if !round.vmce_active() {
        return None;
    }
    let launch = state.match_state.session.network_setup()?;
    let roles = crate::app::shell_network_setup::view::vmce_result_roles(state)?;
    let assignments = state.match_state.session.network_vmce_assignments()?;
    let connected = state.match_state.session.network_connected_source_indices();
    let can_continue = state.match_state.session.cooperative_round_can_continue();
    let victory = round.page_victory();
    let sim = &state.match_state.sim_runtime.as_ref()?.simulation;
    let mut rows = Vec::new();
    let mut humans = launch.humans.iter().enumerate().collect::<Vec<_>>();
    humans.sort_by_key(|(_, human)| human.raw_slot_index.unwrap_or(u8::MAX));
    for (index, human) in humans {
        if human.is_observer() || !connected.contains(&index) {
            continue;
        }
        let Some(assignment) = assignments.participants.iter().find(|assignment| {
            Some(assignment.raw_slot_index) == human.raw_slot_index
                && assignment.participant_name == human.name
        }) else {
            continue;
        };
        let role = usize::from(assignment.player_index);
        if role >= roles.len() {
            continue;
        }
        let info = &roles[role];
        rows.push(VmceResultRow {
            player: human.name.clone(),
            role: info.owner.clone(),
            country: info.country.clone(),
            flag: info.flag.clone(),
            won: sim
                .session()
                .vmce_outcome(sim.interner().get(&info.owner)?)?
                == crate::sim::house_state::HouseOutcomeKind::Victory,
            ready: can_continue.then(|| {
                state
                    .match_state
                    .session
                    .cooperative_round_slot_ready(index)
            }),
            host: human.raw_slot_index == Some(0),
            rgb: info.color,
        });
        if rows.len() == 4 {
            break;
        }
    }
    let local_ready = state
        .match_state
        .session
        .cooperative_round_ready_progress()
        .is_some_and(|(_, _, local)| local);
    (!rows.is_empty()).then_some(VmceResultBoard {
        rows,
        victory,
        local_host: state.match_state.session.host(),
        local_ready,
        can_continue: state.match_state.session.cooperative_round_can_continue(),
    })
}

/// Resolve the next campaign stage. `Ok(None)` is the campaign's real last
/// stage; an `Err` is a broken series and must not be presented as one.
fn resolve_next_stage(
    state: &AppState,
) -> anyhow::Result<crate::app::shell_network_setup::vmce_map::VmceResolvedStage> {
    let Some(current) = state.match_state.session.network_vmce_map_config() else {
        anyhow::bail!("the finished stage has no admitted VMCE series");
    };
    crate::app::shell_network_setup::vmce_map::resolve_next_vmce_stage(state, current)
}

fn leave_after_stage_error(state: &mut AppState) {
    let Some(rows) = score_rows(state) else {
        App::leave_mission_result_screen(state);
        return;
    };
    crate::app::lan_service::cooperative::hold_transport(state);
    App::retire_finished_scenario(state);
    if let Err(error) = state
        .match_state
        .session
        .enter_vmce_round(None, false, rows)
    {
        log::error!("Could not open the VMCE round after a stage error: {error:#}");
        crate::app::cooperative_round::return_to_shell(state);
        return;
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
        Err(error) => {
            log::warn!("VMCE result exit after a stage error was refused: {error:#}");
            crate::app::cooperative_round::return_to_shell(state);
        }
    }
}

fn score_rows(state: &AppState) -> Option<usize> {
    state
        .match_state
        .sim_runtime
        .as_ref()
        .map(|runtime| runtime.simulation.cooperative_score_rows().len())
}

/// Victory statistics use terminal scores; defeat shows the live, unbonused
/// House values.
#[derive(Clone, Copy)]
pub(crate) enum VmceScoreSource {
    Terminal,
    Live,
}

#[allow(clippy::too_many_arguments)]
fn open_page(
    state: &mut AppState,
    model: crate::ui::campaign_score_shell::CampaignScoreScreenModel,
    next: Option<VmceNextStage>,
    can_continue: bool,
    ending: crate::app::match_runtime::session::VmceRoundEnding,
    rows: usize,
    admit_slots: bool,
    what: &str,
    victory: bool,
) {
    let statistics = victory
        .then(|| statistics_page(state, &model, VmceScoreSource::Terminal))
        .flatten();
    state.frontend.score_page = None;
    state.frontend.score_art = None;
    state.frontend.coop_score_page = None;
    crate::app::lan_service::cooperative::hold_transport(state);
    App::retire_finished_scenario(state);
    if let Err(error) =
        state
            .match_state
            .session
            .enter_vmce_round_ending(next, can_continue, ending, rows)
    {
        log::error!("Could not open the VMCE {what} result: {error:#}");
        crate::app::cooperative_round::return_to_shell(state);
        return;
    }
    state
        .match_state
        .session
        .set_cooperative_round_page_victory(victory);
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
    let board = result_board(state);
    // The settlement board's flags/host/ready glyphs come from the skirmish
    // chrome the lobby already loaded; VMCE result pages only exist after it.
    let _ = App::ensure_skirmish_shell_chrome(state);
    App::open_campaign_score_page(state, model, String::new(), String::new());
    super::install_victory_content(state, board, statistics, false);
}

/// Project each admitted player from the frozen assignment and the finished
/// world's House statistics. Both result pages share this owner.
pub(crate) fn statistics_page(
    state: &AppState,
    model: &crate::ui::campaign_score_shell::CampaignScoreScreenModel,
    source: VmceScoreSource,
) -> Option<VmceStatistics> {
    let launch = state.match_state.session.network_setup()?;
    let roles = crate::app::shell_network_setup::view::vmce_result_roles(state);
    let assignments = state.match_state.session.network_vmce_assignments();
    let sim = state
        .match_state
        .sim_runtime
        .as_ref()
        .map(|runtime| &runtime.simulation);
    let terminal = match source {
        VmceScoreSource::Terminal => sim.and_then(|sim| sim.terminal_score_snapshot()),
        VmceScoreSource::Live => None,
    };
    let coop = sim.map(|sim| {
        sim.cooperative_score_rows()
            .into_iter()
            .map(|row| (sim.interner().resolve(row.owner).to_owned(), row))
            .collect::<std::collections::BTreeMap<_, _>>()
    });
    let mut admitted = launch
        .humans
        .iter()
        .filter(|human| !human.is_observer())
        .map(|human| (human.role.native_value(), human))
        .collect::<Vec<_>>();
    admitted.sort_by_key(|(role, human)| (human.raw_slot_index.unwrap_or(u8::MAX), *role));
    let mut rows = Vec::new();
    // Only actual admitted players get a statistics row: a 2-player match
    // shows two rows, a 4-player match four. Country/flag data stays out of
    // the Player column; it is the lobby participant's name.
    for (role, human) in admitted {
        let assignment = assignments.and_then(|assignments| {
            assignments.participants.iter().find(|assignment| {
                Some(assignment.raw_slot_index) == human.raw_slot_index
                    && assignment.participant_name == human.name
            })
        });
        let role = assignment.map_or(role, |assignment| i32::from(assignment.player_index));
        let info = roles
            .as_ref()
            .and_then(|roles| usize::try_from(role).ok().and_then(|role| roles.get(role)));
        let owner = assignment.map_or_else(
            || info.map_or("", |info| info.owner.as_str()),
            |assignment| assignment.owner_name.as_str(),
        );
        let owner_id = sim.and_then(|sim| sim.interner().get(owner));
        let values = terminal.and_then(|snapshot| {
            snapshot
                .rows
                .iter()
                .find(|row| Some(row.owner) == owner_id)
                .map(|row| {
                    (
                        row.kills,
                        row.losses,
                        row.built,
                        row.score,
                        row.credits_earned,
                    )
                })
        });
        let (kills, losses, built, score, income) = values
            .or_else(|| {
                coop.as_ref()?.get(owner).map(|row| {
                    (
                        row.kills,
                        row.losses,
                        row.built,
                        row.score,
                        row.credits_earned,
                    )
                })
            })
            .unwrap_or((0, 0, 0, 0, 0));
        rows.push(VmceStatisticsRow {
            player: human.name.clone(),
            owner: owner.to_owned(),
            rgb: info.and_then(|info| info.color),
            kills,
            losses,
            built,
            score,
            income,
        });
    }
    Some(VmceStatistics {
        rows,
        total_time: bare_duration(&model.your_time),
        par_time: bare_duration(&model.par_time),
    })
}

/// The native score model's time fields keep their CSF label (`時間:...`).
/// The shared statistics header draws its own label, so the value is only the
/// trailing `HH:MM:SS`.
fn bare_duration(value: &str) -> String {
    let value = value.trim();
    let candidate = value
        .char_indices()
        .rev()
        .nth(7)
        .map_or(value, |(index, _)| &value[index..]);
    let bytes = candidate.as_bytes();
    if candidate.len() == 8
        && bytes[2] == b':'
        && bytes[5] == b':'
        && bytes[..2]
            .iter()
            .chain(&bytes[3..5])
            .all(u8::is_ascii_digit)
        && bytes[6..].iter().all(u8::is_ascii_digit)
    {
        candidate.to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::vmce_result::types;

    #[test]
    fn the_statistics_time_header_drops_the_native_label() {
        assert_eq!(bare_duration("時間:00:00:13"), "00:00:13");
        assert_eq!(bare_duration("00:00:13"), "00:00:13");
        assert_eq!(bare_duration(""), "");
    }

    #[test]
    fn the_result_row_text_uses_the_authored_house_colour() {
        let mut row = VmceResultRow {
            player: "LiveHost".into(),
            role: "Americans House".into(),
            country: "America".into(),
            flag: None,
            won: true,
            ready: Some(true),
            host: true,
            rgb: Some([32, 160, 64]),
        };
        assert_eq!(types::result_text_rgb(&row), [32, 160, 64]);
        row.rgb = None;
        assert_eq!(
            types::result_text_rgb(&row),
            [255, 255, 0],
            "a role without an authored colour keeps the native yellow"
        );
    }

    #[test]
    fn the_statistics_subpage_state_is_page_local_and_reopenable() {
        let mut page = VictoryResultPage::new(None, None);
        page.toggle_statistics();
        assert!(!page.statistics_open(), "no statistics, no page");
        page.statistics = Some(VmceStatistics {
            rows: Vec::new(),
            total_time: "00:01:00".into(),
            par_time: "00:02:00".into(),
        });
        page.toggle_statistics();
        assert!(page.statistics_open(), "the tab opens the page");
        page.toggle_statistics();
        assert!(!page.statistics_open(), "the same tab toggles back");
        page.toggle_statistics();
        page.close_statistics();
        assert!(!page.statistics_open(), "Back returns to the board");
        // A reopened page starts clean: a fresh container carries no subpage
        // state from the previous page instance.
        let reopened = VictoryResultPage::new(None, None);
        assert!(!reopened.statistics_open());
    }

    #[test]
    fn a_refresh_reprojects_the_board() {
        let mut page = VictoryResultPage::new(
            None,
            Some(VmceStatistics {
                rows: Vec::new(),
                total_time: String::new(),
                par_time: String::new(),
            }),
        );
        assert!(!page.statistics_open());
        let board = VmceResultBoard {
            rows: vec![VmceResultRow {
                player: "Guest".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        page.refresh(Some(board));
        assert_eq!(page.board.as_ref().map(|board| board.rows.len()), Some(1));
    }
}
