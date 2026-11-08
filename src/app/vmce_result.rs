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

/// Open the result page for a lost mission. Continue retries the same
/// frozen mission on every peer; no next stage is carried.
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
    open_page(
        state,
        model,
        None,
        true,
        crate::app::match_runtime::session::VmceRoundEnding::Retry,
        rows,
        false,
        "defeat",
        false,
    );
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

/// The settlement board the `0x103` page shows: one row per connected human
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
            role: crate::i18n::tr_fill(
                "VERA:VmceResultRole",
                &[("number", &(role + 1)), ("house", &info.owner)],
            ),
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
        });
        if rows.len() == 4 {
            break;
        }
    }
    let (ready, total, _) = state
        .match_state
        .session
        .cooperative_round_ready_progress()
        .unwrap_or((0, rows.len(), false));
    let host = state.match_state.session.host();
    let caption = continuation_caption(round.vera_ending(), host, ready, total);
    (!rows.is_empty()).then_some(VmceResultBoard {
        rows,
        victory,
        caption,
    })
}

/// The truthful status caption for one continuation. The three continuations
/// keep their own gates in the round; this only states what the page does.
#[cfg(feature = "vmce")]
fn continuation_caption(
    ending: crate::app::match_runtime::session::VmceRoundEnding,
    host: bool,
    ready: usize,
    total: usize,
) -> String {
    use crate::app::match_runtime::session::VmceRoundEnding;
    match ending {
        VmceRoundEnding::Next => crate::i18n::tr_fill(
            "VERA:VmceResultNextStatus",
            &[("ready", &ready), ("total", &total)],
        ),
        VmceRoundEnding::Retry => {
            if host {
                crate::i18n::tr("VERA:VmceResultRetryHostStatus")
            } else {
                crate::i18n::tr("VERA:VmceResultRetryGuestStatus")
            }
        }
        VmceRoundEnding::CampaignEnd => crate::i18n::tr_fill(
            "VERA:VmceResultEndStatus",
            &[("ready", &ready), ("total", &total)],
        ),
    }
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
    }
}

#[cfg(all(test, feature = "vmce"))]
mod tests {
    use super::*;
    use crate::app::match_runtime::session::VmceRoundEnding;

    #[test]
    fn the_three_continuations_get_their_own_truthful_captions() {
        let next = continuation_caption(VmceRoundEnding::Next, true, 2, 4);
        let retry_host = continuation_caption(VmceRoundEnding::Retry, true, 0, 4);
        let retry_guest = continuation_caption(VmceRoundEnding::Retry, false, 0, 4);
        let end = continuation_caption(VmceRoundEnding::CampaignEnd, true, 4, 4);
        assert!(
            next.contains("2/4"),
            "next caption carries its ready gate: {next}"
        );
        assert!(
            end.contains("4/4"),
            "campaign-end caption carries its ready gate: {end}"
        );
        assert!(
            next.contains("all are ready") || next.contains("全员就绪"),
            "next caption states the all-ready gate: {next}"
        );
        assert!(
            retry_host.contains("15") && retry_guest.contains("15"),
            "retry captions carry the grace: {retry_host} / {retry_guest}"
        );
        assert_ne!(next, end);
        assert_ne!(retry_host, retry_guest);
        assert_ne!(next, retry_host);
    }
}
