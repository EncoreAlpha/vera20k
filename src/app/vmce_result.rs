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

/// Open the defeat page on the native BC6 shell (`0xBC6`), not `0x103`. The
/// page is the statistics view itself: four role rows, no chat and no player
/// setup columns. Its Continue is the retry vote/start gate; its Quit is
/// Leave Room.
pub(crate) fn open_defeat(state: &mut AppState) {
    let elapsed = state
        .match_state
        .scenario_elapsed_clock
        .stop_buckets(crate::app::match_runtime::frame_pacer::wall_clock_ms());
    let Some(mut model) = crate::app::cooperative_round::build_vmce_defeat_model(state, elapsed)
    else {
        App::leave_mission_result_screen(state);
        return;
    };
    // The failure page shows the same statistics table as the victory stats
    // page: score rows plus the gross-income accumulator and the stage's par
    // time. Project it while the finished world is still resident.
    let ranking_model =
        crate::app::match_runtime::sim_tick::build_vmce_victory_score_model(state, elapsed);
    model.vmce_statistics = statistics_page(state, &ranking_model);
    let rows = model.rows.len();
    let side = model.side;
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
    let _ = side;
    open_defeat_page(state, model);
}

/// Present an already-built VMCE defeat model on the native BC6 shell. Both
/// the real defeat path and the synthetic captures use this one page owner.
#[cfg(feature = "vmce")]
pub(crate) fn open_defeat_page(
    state: &mut AppState,
    model: crate::ui::coop_score_shell::CoopScoreModel,
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
    state.frontend.screen = crate::app::GameScreen::MissionResult {
        title: String::new(),
        detail: String::new(),
    };
    state
        .frontend
        .shell_controller
        .reset_to(crate::ui::coop_score_shell::COOP_SCORE_DIALOG, false);
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

/// The settlement board the `0x103` page shows: one row per connected human/// The settlement board the `0x103` page shows: one row per connected human
/// with its authored role, country and verdict, plus the truthful caption for
/// the continuation this page gates. Read-only projection of the round and
/// launch the page already owns; no second result state.
#[cfg(feature = "vmce")]
pub(crate) fn result_board(
    state: &AppState,
) -> Option<crate::ui::campaign_score_shell::VmceResultBoard> {
    use crate::ui::campaign_score_shell::{VmceResultBoard, VmceResultRow};
    let round = state.match_state.session.cooperative_round()?;
    if !round.vera() {
        return None;
    }
    let launch = state.match_state.session.network_setup()?;
    let roles = crate::app::shell_network_setup::view::vmce_result_roles(state)?;
    let connected = state.match_state.session.network_connected_source_indices();
    let can_continue = state.match_state.session.cooperative_round_can_continue();
    let victory = round.page_victory();
    // Per-row verdict: the mission viewpoint's own survival when the finished
    // world is still resident; otherwise the page verdict every peer shows.
    let mut survived = std::collections::BTreeMap::new();
    if let Some(runtime) = state.match_state.sim_runtime.as_ref() {
        let sim = &runtime.simulation;
        for row in sim.cooperative_score_rows() {
            survived.insert(sim.interner().resolve(row.owner).to_owned(), row.survived);
        }
    }
    let mut rows = Vec::new();
    for (index, human) in launch.humans.iter().enumerate() {
        if human.is_observer() || !connected.contains(&index) {
            continue;
        }
        let role = human.role.native_value();
        if role < 0 {
            continue;
        }
        let role = role as usize;
        if role >= roles.len() {
            continue;
        }
        let info = &roles[role];
        rows.push(VmceResultRow {
            player: human.name.clone(),
            role: info.owner.clone(),
            country: info.country.clone(),
            flag: info.flag.clone(),
            won: survived
                .get(info.owner.as_str())
                .copied()
                .unwrap_or(victory),
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
    #[cfg(feature = "vmce")]
    let statistics = victory.then(|| statistics_page(state, &model)).flatten();
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
    #[cfg(feature = "vmce")]
    let board = result_board(state);
    // The settlement board's flags/host/ready glyphs come from the skirmish
    // chrome the lobby already loaded; VMCE result pages only exist after it.
    #[cfg(feature = "vmce")]
    let _ = App::ensure_skirmish_shell_chrome(state);
    App::open_campaign_score_page(state, model, String::new(), String::new());
    #[cfg(feature = "vmce")]
    if let Some(page) = state.frontend.campaign_score_page.as_mut() {
        page.set_vmce_board(board);
        page.set_vmce_statistics(statistics);
    }
}

/// VERA's victory/campaign-end statistics projection: the B25 player/House
/// rows plus the terminal score cells and the mission income statistic. Read
/// only; no second result state.
#[cfg(feature = "vmce")]
pub(crate) fn statistics_page(
    state: &AppState,
    model: &crate::ui::campaign_score_shell::CampaignScoreScreenModel,
) -> Option<crate::ui::campaign_score_shell::VmceStatistics> {
    use crate::ui::campaign_score_shell::{VmceStatistics, VmceStatisticsRow};
    let launch = state.match_state.session.network_setup()?;
    let roles = crate::app::shell_network_setup::view::vmce_result_roles(state)?;
    let sim = state
        .match_state
        .sim_runtime
        .as_ref()
        .map(|runtime| &runtime.simulation);
    let terminal = sim.and_then(|sim| sim.terminal_score_snapshot());
    let coop = sim.map(|sim| {
        sim.cooperative_score_rows()
            .into_iter()
            .map(|row| (sim.interner().resolve(row.owner).to_owned(), row))
            .collect::<std::collections::BTreeMap<_, _>>()
    });
    let mut rows = Vec::new();
    for (role, info) in roles.iter().enumerate() {
        if info.owner.is_empty() {
            continue;
        }
        let player = launch
            .humans
            .iter()
            .find(|human| !human.is_observer() && human.role.native_value() == role as i32)
            .map_or_else(|| info.owner.clone(), |human| human.name.clone());
        let owner_id = sim
            .and_then(|sim| sim.interner().get(&info.owner))
            .or_else(|| sim.and_then(|sim| sim.interner().get(&info.owner)));
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
        let (kills, losses, built, score, income) = values.or_else(|| {
            coop.as_ref()?.get(&info.owner).map(|row| {
                (
                    row.kills,
                    row.losses,
                    row.built,
                    row.score,
                    row.credits_earned,
                )
            })
        })?;
        rows.push(VmceStatisticsRow {
            player,
            owner: info.owner.clone(),
            rgb: info.color,
            kills,
            losses,
            built,
            score,
            income,
        });
    }
    (!rows.is_empty()).then(|| VmceStatistics {
        rows,
        total_time: model.your_time.clone(),
        par_time: model.par_time.clone(),
    })
}

#[cfg(all(test, feature = "vmce"))]
mod tests {
    use super::*;
    use crate::app::match_runtime::session::VmceRoundEnding;

    #[test]
    fn the_result_row_text_uses_the_authored_house_colour() {
        let mut row = crate::ui::campaign_score_shell::VmceResultRow {
            player: "LiveHost".into(),
            role: "Americans House".into(),
            country: "America".into(),
            flag: None,
            won: true,
            ready: Some(true),
            host: true,
            rgb: Some([32, 160, 64]),
        };
        assert_eq!(
            crate::ui::campaign_score_shell::vmce_result_text_rgb(&row),
            [32, 160, 64]
        );
        row.rgb = None;
        assert_eq!(
            crate::ui::campaign_score_shell::vmce_result_text_rgb(&row),
            [255, 255, 0],
            "a role without an authored colour keeps the native yellow"
        );
    }
}
