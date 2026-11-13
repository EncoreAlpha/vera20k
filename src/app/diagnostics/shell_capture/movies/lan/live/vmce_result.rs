//! VMCE's LAN result diagnostic: the `--vera-outcome` tail and its
//! real-trigger variants observe VMCE's own settlement pages.
//!
//! A won or finished campaign opens the stock campaign score dialog
//! `0x103` carrying VMCE's victory content (`campaign_score_page` plus
//! `frontend.vmce_result_page`); a lost mission opens VMCE's own
//! defeat/retry page. This diagnostic waits for exactly those pages, reads
//! VMCE's statistics and drives VMCE's own result actions. It never drives
//! or waits for a stock multiplayer result page that VMCE does not own: the
//! VMCE result owners open their own page, and the stock Cooperative
//! diagnostics in `live/cooperative.rs` keep their own flow.

use super::*;
use crate::app::vmce_result::defeat::{self, VmceDefeatAction};
use crate::app::vmce_result::victory::VictoryResultPage;

/// Both peers use the same absolute frame so the injected outcome enters the
/// shared world at the same native frame, not at a per-process readiness edge.
const VERA_OUTCOME_MISSION_SHOT_FRAME: u32 = 400;
const VERA_OUTCOME_INJECT_FRAME: u32 = 500;
/// Render-frame budget for a real map trigger chain. Authored delays can be
/// minutes of native time (the test map's chain is 20 s + 4 s / 30 s + 12 s).
const VERA_OUTCOME_REAL_TIMEOUT_FRAMES: u32 = 4_000;
/// Render frames the continued mission must stay alive after a House loss.
const VERA_OUTCOME_CONTINUED_FRAMES: u32 = 150;

/// Which VMCE result page a settled mission is showing. A pure projection
/// of the app's page slots and the network visit, so unit tests cover the
/// detection pattern without building a window, GPU or network session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VmceResultPageKind {
    /// Stock `0x103` campaign score dialog carrying VMCE victory content.
    Victory,
    /// VMCE's own defeat/retry page.
    Defeat,
    /// No VMCE result page is open: the mission still runs, or a stock
    /// page that VMCE does not own is showing.
    None,
}

/// The page slots [`VmceResultPageKind`] reads, taken from `AppState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct VmceResultPageView {
    /// The stock `0x103` dialog is open and hosts VMCE's victory page.
    campaign_score_page: bool,
    /// The `0x103` page carries VMCE's settlement board.
    victory_board: bool,
    /// The `0x103` page carries VMCE's statistics projection.
    victory_statistics: bool,
    /// The network visit still holds VMCE's active result phase.
    result_round: bool,
    /// VMCE's own defeat/retry page is open.
    defeat_page: bool,
    /// The defeat page carries its statistics rows and time headers.
    defeat_statistics: bool,
}

impl VmceResultPageView {
    /// Detect VMCE's own result pages. A stock result page that VMCE does
    /// not decorate (including the stock multiplayer `0xBC6`) leaves every slot
    /// here unset, so the diagnostic never waits for it.
    fn kind(self) -> VmceResultPageKind {
        if self.campaign_score_page
            && self.victory_board
            && self.victory_statistics
            && self.result_round
        {
            VmceResultPageKind::Victory
        } else if self.defeat_page && self.defeat_statistics {
            VmceResultPageKind::Defeat
        } else {
            VmceResultPageKind::None
        }
    }
}

/// A time header the VMCE statistics page can show: `HH:MM:SS` without
/// its native label. The defeat detector requires both headers.
fn vmce_duration_header(value: &str) -> bool {
    value.len() == 8 && value.matches(':').count() == 2
}

fn result_page_view(state: &AppState) -> VmceResultPageView {
    let victory = state
        .frontend
        .vmce_result_page
        .as_ref()
        .and_then(|page| page.victory());
    let defeat = defeat::page(state);
    VmceResultPageView {
        campaign_score_page: state.frontend.campaign_score_page.is_some(),
        victory_board: victory.is_some_and(|page| page.board.is_some()),
        victory_statistics: victory.is_some_and(|page| {
            page.statistics
                .as_ref()
                .is_some_and(|statistics| !statistics.rows.is_empty())
        }),
        result_round: state
            .match_state
            .session
            .vmce_result_phase()
            .is_some_and(|round| round.vmce_active()),
        defeat_page: defeat.is_some(),
        defeat_statistics: defeat.is_some_and(|page| {
            !page.statistics.rows.is_empty()
                && vmce_duration_header(&page.statistics.total_time)
                && vmce_duration_header(&page.statistics.par_time)
        }),
    }
}

/// The VMCE result page currently shown, if any.
fn result_page(state: &AppState) -> VmceResultPageKind {
    result_page_view(state).kind()
}

/// The open VMCE victory page, when `0x103` carries its content and the
/// result round is active. Reading VMCE's own accessors keeps this
/// diagnostic independent of every stock multiplayer result page.
fn victory_page(state: &AppState) -> Option<&VictoryResultPage> {
    if result_page(state) != VmceResultPageKind::Victory {
        return None;
    }
    state
        .frontend
        .vmce_result_page
        .as_ref()
        .and_then(|page| page.victory())
}

impl LiveLanCapture {
    /// After the injection the app retires the mission on its own; this tail
    /// only observes the result dialog (or the continued mission) in render
    /// frames, so it keeps running while the shared simulation is paused.
    pub(super) fn finish_vera_outcome(&mut self, state: &mut AppState) -> Result<()> {
        self.vera_outcome_render_frames = self.vera_outcome_render_frames.saturating_add(1);
        let outcome = self.vera_outcome.context("VERA outcome diagnostic")?;
        let kind = outcome.as_str();
        let render_frame = self.vera_outcome_render_frames;
        match self.vera_outcome_phase {
            VeraOutcomePhase::Observe => match outcome {
                VmceOutcome::Win | VmceOutcome::Lose => {
                    // Win waits for VMCE's victory content on the stock `0x103`
                    // campaign score dialog; lose waits for VMCE's own defeat
                    // page. The VMCE result owners never open the stock
                    // stock BC6 page, so that slot would never appear.
                    let expected = match outcome {
                        VmceOutcome::Win => VmceResultPageKind::Victory,
                        _ => VmceResultPageKind::Defeat,
                    };
                    if result_page(state) == expected {
                        let (victory, rows, can_continue) = match outcome {
                            VmceOutcome::Win => {
                                let page = victory_page(state).context("VMCE victory page")?;
                                let board = page
                                    .board
                                    .as_ref()
                                    .context("VMCE victory page has no settlement board")?;
                                ensure!(board.victory, "VMCE victory page reported a defeat board");
                                let statistics = page
                                    .statistics
                                    .as_ref()
                                    .context("VMCE victory statistics are missing")?;
                                ensure!(
                                    statistics.rows.len() == self.expected_players,
                                    "VMCE victory statistics rows do not match admitted players: {} != {}",
                                    statistics.rows.len(),
                                    self.expected_players
                                );
                                (true, statistics.rows.len(), board.can_continue)
                            }
                            _ => {
                                let page = defeat::page(state).context("VMCE defeat page")?;
                                ensure!(
                                    page.statistics.rows.len() == self.expected_players,
                                    "VMCE defeat statistics rows do not match admitted players: {} != {}",
                                    page.statistics.rows.len(),
                                    self.expected_players
                                );
                                (false, page.statistics.rows.len(), true)
                            }
                        };
                        self.vera_outcome_evidence = Some(json!({
                            "kind": kind,
                            "injected_frame": self.vera_outcome_injected_frame,
                            "dialog": {
                                "victory": victory,
                                "rows": rows,
                                "can_continue": can_continue,
                                "render_frame": render_frame,
                            },
                        }));
                        self.request_screenshot(format!("vera-result-{kind}"));
                        if let (Some(receipt), Some(evidence)) = (
                            self.final_receipt.as_mut(),
                            self.vera_outcome_evidence.as_ref(),
                        ) {
                            receipt["vera_outcome"] = evidence.clone();
                        }
                        self.event(
                            render_frame,
                            if victory {
                                "VERA outcome: VMCE victory page opened on campaign score 0x103"
                            } else {
                                "VERA outcome: VMCE defeat page opened"
                            },
                        );
                        self.vera_outcome_dialog_frame = Some(render_frame);
                        self.vera_outcome_phase = VeraOutcomePhase::Quit;
                    } else {
                        ensure!(
                            render_frame < 900,
                            "VMCE result page did not open after the injected outcome: expected {expected:?}, current {:?}, screen={:?}",
                            result_page(state),
                            state.frontend.screen
                        );
                    }
                }
                VmceOutcome::Real | VmceOutcome::End | VmceOutcome::Retry => {
                    // End/retry use the same 0x103 page as a real win or
                    // defeat; any VMCE page here is the one to continue.
                    // Escape on the VMCE defeat page does not leave the
                    // room. Exercise the production input handler once the
                    // defeat page is actually open, then prove the page and
                    // round are still open.
                    if crate::app::vmce_result::defeat::page(state).is_some() {
                        ensure!(
                            !crate::app::frontend::shell_transition::blocks_shell_input(state),
                            "VMCE defeat page input was unexpectedly blocked"
                        );
                        ensure!(
                            crate::app::handler::handle_vmce_defeat_key_code(
                                state,
                                winit::keyboard::KeyCode::Escape,
                            ),
                            "the window dispatch did not route Escape to the VMCE defeat page"
                        );
                        ensure!(
                            crate::app::vmce_result::defeat::page(state).is_some(),
                            "Escape dismissed the VMCE defeat page"
                        );
                        ensure!(
                            state.match_state.session.vmce_result_phase().is_some(),
                            "Escape left the VMCE result round"
                        );
                    }
                    // VMCE's own page accessors: the victory page is the
                    // `0x103` dialog carrying VMCE content while the result
                    // round is active; defeat is VMCE's own page below.
                    let victory_page = victory_page(state);
                    if let Some(defeat_page) = crate::app::vmce_result::defeat::page(state) {
                        let statistics = &defeat_page.statistics;
                        ensure!(
                            statistics.rows.len() == self.expected_players,
                            "VMCE defeat rows do not match admitted players: {} != {}",
                            statistics.rows.len(),
                            self.expected_players
                        );
                        for time in [statistics.total_time.as_str(), statistics.par_time.as_str()] {
                            ensure!(
                                time.len() == 8 && time.matches(':').count() == 2,
                                "VMCE defeat time header embeds its label: {time:?}"
                            );
                        }
                        let launch = state
                            .match_state
                            .session
                            .network_setup()
                            .context("VMCE defeat roster")?;
                        let mut expected = launch
                            .humans
                            .iter()
                            .filter(|human| !human.is_observer())
                            .collect::<Vec<_>>();
                        expected.sort_by_key(|human| {
                            (
                                human.raw_slot_index.unwrap_or(u8::MAX),
                                human.role.native_value(),
                            )
                        });
                        ensure!(
                            statistics
                                .rows
                                .iter()
                                .map(|row| row.player.as_str())
                                .eq(expected.iter().map(|human| human.name.as_str())),
                            "VMCE defeat rows are not in admission order: {:?}",
                            statistics
                                .rows
                                .iter()
                                .map(|row| row.player.as_str())
                                .collect::<Vec<_>>()
                        );
                        ensure!(
                            statistics.rows.iter().any(|row| {
                                row.kills != 0
                                    || row.losses != 0
                                    || row.built != 0
                                    || row.score != 0
                                    || row.income != 0
                            }),
                            "VMCE defeat table zeroed every live score row"
                        );
                        let assignments = state
                            .match_state
                            .session
                            .network_vmce_assignments()
                            .context("VMCE defeat assignments")?;
                        let sim = &state
                            .match_state
                            .sim_runtime
                            .runtime()
                            .context("VMCE defeat simulation")?
                            .simulation;
                        let score_rows = sim.cooperative_score_rows();
                        let first_seat = assignments
                            .participants
                            .iter()
                            .min_by_key(|assignment| assignment.raw_slot_index)
                            .context("VMCE defeat has no participants")?;
                        ensure!(
                            score_rows
                                .first()
                                .is_some_and(|score| sim.interner().resolve(score.owner)
                                    != first_seat.owner_name),
                            "retry probe did not reverse the actual score and seat order"
                        );
                        let roles = crate::app::shell_network_setup::view::vmce_result_roles(state)
                            .context("VMCE defeat role metadata")?;
                        for row in &statistics.rows {
                            let assignment = assignments
                                .participants
                                .iter()
                                .find(|assignment| assignment.participant_name == row.player)
                                .context("VMCE defeat player has no frozen assignment")?;
                            ensure!(
                                row.owner == assignment.owner_name
                                    && row.rgb == roles[usize::from(assignment.player_index)].color,
                                "VMCE defeat player {} has the wrong House or colour",
                                row.player
                            );
                            let score = score_rows
                                .iter()
                                .find(|score| {
                                    sim.interner().resolve(score.owner) == assignment.owner_name
                                })
                                .context("VMCE defeat House has no score row")?;
                            ensure!(
                                (row.kills, row.losses, row.built, row.score, row.income)
                                    == (
                                        score.kills,
                                        score.losses,
                                        score.built,
                                        score.score,
                                        score.credits_earned
                                    ),
                                "VMCE defeat statistics for {} do not match House {}",
                                row.player,
                                assignment.owner_name
                            );
                        }
                        let rows_json = statistics
                            .rows
                            .iter()
                            .map(|row| {
                                json!({
                                    "player": row.player,
                                    "kills": row.kills,
                                    "losses": row.losses,
                                    "built": row.built,
                                    "score": row.score,
                                    "income": row.income,
                                })
                            })
                            .collect::<Vec<_>>();
                        if let Some(receipt) = self.final_receipt.as_mut() {
                            receipt["vmce_statistics_rows"] = json!(rows_json);
                        }
                        if crate::i18n::selected_language() == "zh-CN" {
                            let (width, height) =
                                crate::app::frontend::shell_layout::shell_screen_size(state);
                            let scale =
                                crate::app::vmce_result::defeat_layout::VmceDefeatLayout::new(
                                    width as i32,
                                    height as i32,
                                    crate::app::frontend::shell_layout::fills_window(state),
                                )
                                .scale();
                            let layout = crate::app::vmce_result::types::statistics_layout(scale);
                            for header in layout.headers {
                                let label = if let Some(key) = header.key.strip_prefix("VERA:") {
                                    crate::i18n::tr(&format!("VERA:{key}"))
                                } else {
                                    crate::i18n::label(
                                        state.process_assets.csf.as_ref(),
                                        header.key,
                                        header.key,
                                    )
                                };
                                ensure!(
                                    label.chars().any(|character| !character.is_ascii()),
                                    "VMCE defeat header is not localized in zh-CN: {label:?}"
                                );
                            }
                            ensure!(
                                crate::i18n::tr("VERA:VmceIncome") == "收入",
                                "VMCE income header is not localized in zh-CN"
                            );
                        }
                        let victory = false;
                        let rows = statistics.rows.len();
                        let can_continue = true;
                        ensure!(rows > 0, "VMCE real outcome page has no role rows");
                        ensure!(
                            rows == self.expected_players,
                            "VMCE result rows do not match the admitted players"
                        );
                        let native_frame = state
                            .match_state
                            .sim_runtime
                            .runtime()
                            .map(|runtime| runtime.simulation.session().binary_frame());
                        let state_hash = state
                            .match_state
                            .sim_runtime
                            .runtime()
                            .map(|runtime| runtime.simulation.state_hash());
                        let local_ready = state
                            .match_state
                            .session
                            .vmce_result_phase()
                            .is_some_and(|round| round.local_ready());
                        let slot_ready: Vec<bool> = (0..4)
                            .map(|index| state.match_state.session.vmce_result_slot_ready(index))
                            .collect();
                        let continue_enabled =
                            state.match_state.session.vmce_result_continue_enabled();
                        self.vera_outcome_evidence = Some(json!({
                            "kind": kind,
                            "injected_frame": json!(null),
                            "dialog": {
                                "victory": victory,
                                "rows": rows,
                                "time": statistics.total_time,
                                "can_continue": can_continue,
                                "render_frame": render_frame,
                            },
                            "round": {
                                "local_ready": local_ready,
                                "slot_ready": slot_ready,
                                "continue_enabled": continue_enabled,
                            },
                            "native_frame": native_frame,
                            "state_hash": state_hash,
                            "escape_preserved_page": true,
                        }));
                        if let (Some(receipt), Some(evidence)) = (
                            self.final_receipt.as_mut(),
                            self.vera_outcome_evidence.as_ref(),
                        ) {
                            receipt["vera_outcome"] = evidence.clone();
                        }
                        self.event(
                            render_frame,
                            "VERA outcome: map's real trigger chain opened the VMCE defeat page",
                        );
                        self.vera_outcome_dialog_frame = Some(render_frame);
                        self.vera_outcome_phase = if can_continue {
                            VeraOutcomePhase::Round
                        } else {
                            VeraOutcomePhase::Quit
                        };
                    } else if let Some(page) = victory_page {
                        // VMCE's won mission uses the native campaign score
                        // page 0x103; the lost mission opened VMCE's own
                        // defeat page above.
                        let rows = state
                            .match_state
                            .sim_runtime
                            .runtime()
                            .map(|runtime| runtime.simulation.cooperative_score_rows().len())
                            .unwrap_or(0);
                        ensure!(rows > 0, "VMCE real outcome page has no role rows");
                        ensure!(
                            rows == self.expected_players,
                            "VMCE result rows do not match the admitted players"
                        );
                        let can_continue = state.match_state.session.vmce_result_can_continue();
                        let victory = state
                            .match_state
                            .session
                            .vmce_result_phase()
                            .is_some_and(|round| round.page_victory());
                        let native_frame = state
                            .match_state
                            .sim_runtime
                            .runtime()
                            .map(|runtime| runtime.simulation.session().binary_frame());
                        let state_hash = state
                            .match_state
                            .sim_runtime
                            .runtime()
                            .map(|runtime| runtime.simulation.state_hash());
                        let local_ready = state
                            .match_state
                            .session
                            .vmce_result_phase()
                            .is_some_and(|round| round.local_ready());
                        let slot_ready: Vec<bool> = (0..4)
                            .map(|index| state.match_state.session.vmce_result_slot_ready(index))
                            .collect();
                        let continue_enabled =
                            state.match_state.session.vmce_result_continue_enabled();
                        self.vera_outcome_evidence = Some(json!({
                            "kind": kind,
                            "injected_frame": json!(null),
                            "dialog": {
                                "victory": victory,
                                "dialog": 0x103,
                                "rows": rows,
                                "time": page
                                    .statistics
                                    .as_ref()
                                    .map_or("", |statistics| statistics.total_time.as_str()),
                                "can_continue": can_continue,
                                "render_frame": render_frame,
                            },
                            "round": {
                                "local_ready": local_ready,
                                "slot_ready": slot_ready,
                                "continue_enabled": continue_enabled,
                            },
                            "native_frame": native_frame,
                            "state_hash": state_hash,
                        }));
                        if let (Some(receipt), Some(evidence)) = (
                            self.final_receipt.as_mut(),
                            self.vera_outcome_evidence.as_ref(),
                        ) {
                            receipt["vera_outcome"] = evidence.clone();
                        }
                        self.event(
                            render_frame,
                            "VERA outcome: map's real trigger chain opened VMCE's campaign score 0x103 page",
                        );
                        self.vera_outcome_dialog_frame = Some(render_frame);
                        self.vera_outcome_phase = if can_continue {
                            VeraOutcomePhase::Round
                        } else {
                            VeraOutcomePhase::Quit
                        };
                    } else {
                        ensure!(
                            render_frame < VERA_OUTCOME_REAL_TIMEOUT_FRAMES,
                            "VMCE real trigger outcome did not open the result dialog: render_frame={render_frame}"
                        );
                    }
                }
                VmceOutcome::HouseLoss => {
                    ensure!(
                        state.frontend.screen == GameScreen::InGame
                            && result_page(state) == VmceResultPageKind::None
                            && state.match_state.scenario_exit.is_none(),
                        "VMCE ended after every player House was marked defeated"
                    );
                    if render_frame >= VERA_OUTCOME_CONTINUED_FRAMES {
                        let continued_native_frame = state
                            .match_state
                            .sim_runtime
                            .runtime()
                            .map(|runtime| runtime.simulation.session().binary_frame());
                        self.vera_outcome_evidence = Some(json!({
                            "kind": kind,
                            "injected_frame": self.vera_outcome_injected_frame,
                            "continued_render_frames": render_frame,
                            "continued_native_frame": continued_native_frame,
                            "marked_houses": self.vera_outcome_houses,
                            "mission_running": true,
                        }));
                        self.request_screenshot("vera-continued");
                        if let (Some(receipt), Some(evidence)) = (
                            self.final_receipt.as_mut(),
                            self.vera_outcome_evidence.as_ref(),
                        ) {
                            receipt["vera_outcome"] = evidence.clone();
                        }
                        self.event(
                            render_frame,
                            "VERA outcome: mission kept running after every player House was marked defeated",
                        );
                        self.phase = Phase::Finished;
                    }
                }
            },
            VeraOutcomePhase::Round => {
                let Some(opened) = self.vera_outcome_dialog_frame else {
                    anyhow::bail!("VERA result dialog frame missing");
                };
                if render_frame < opened.saturating_add(30) {
                    return Ok(());
                }
                // The score page hides its children during the entry wave;
                // an early screenshot cannot establish what the player sees.
                if crate::app::frontend::shell_transition::shell_slide_running(state) {
                    ensure!(
                        render_frame < opened.saturating_add(1_200),
                        "VMCE result entry animation did not finish"
                    );
                    return Ok(());
                }
                if !self.vera_result_shot_done {
                    // The screenshot label follows VMCE's own page verdict;
                    // the round's committed verdict is the fallback while a
                    // page is between its stock host and VMCE content.
                    let victory = match result_page(state) {
                        VmceResultPageKind::Victory => true,
                        VmceResultPageKind::Defeat => false,
                        VmceResultPageKind::None => state
                            .match_state
                            .session
                            .vmce_result_phase()
                            .is_some_and(|round| round.page_victory()),
                    };
                    self.request_screenshot(if victory {
                        "vera-result-real-win"
                    } else {
                        "vera-result-real-lose"
                    });
                    self.vera_result_shot_done = true;
                    return Ok(());
                }
                let victory_page = victory_page(state);
                if victory_page.is_some() {
                    let board = state
                        .frontend
                        .vmce_result_page
                        .as_ref()
                        .and_then(|page| page.victory())
                        .and_then(|page| page.board.as_ref())
                        .context("settled VMCE victory page has no result board")?;
                    ensure!(
                        board.rows.len() == self.expected_players,
                        "settled VMCE victory board has {} rows; expected {}",
                        board.rows.len(),
                        self.expected_players
                    );
                    let session = &state.match_state.session;
                    let launch = session.network_setup().context("VMCE result launch")?;
                    let assignments = session
                        .network_vmce_assignments()
                        .context("VMCE result assignments")?;
                    for row in &board.rows {
                        let (index, human) = launch
                            .humans
                            .iter()
                            .enumerate()
                            .find(|(_, human)| human.name == row.player)
                            .context("VMCE result player is not admitted")?;
                        let assignment = assignments
                            .participants
                            .iter()
                            .find(|assignment| {
                                Some(assignment.raw_slot_index) == human.raw_slot_index
                                    && assignment.participant_name == human.name
                            })
                            .context("VMCE result player has no frozen assignment")?;
                        ensure!(
                            row.role == assignment.owner_name,
                            "VMCE result player {} has the wrong House",
                            row.player
                        );
                        ensure!(
                            row.ready == Some(session.vmce_result_slot_ready(index)),
                            "VMCE result player {} has stale Ready status",
                            row.player
                        );
                    }
                    ensure!(
                        board.local_ready
                            == session
                                .vmce_result_ready_progress()
                                .is_some_and(|(_, _, local)| local),
                        "VMCE result local Ready status is stale"
                    );
                    let statistics = state
                        .frontend
                        .vmce_result_page
                        .as_ref()
                        .and_then(|page| page.statistics())
                        .context("VMCE victory statistics are missing")?;
                    let sim = &state
                        .match_state
                        .sim_runtime
                        .runtime()
                        .context("VMCE victory simulation")?
                        .simulation;
                    let terminal = sim.terminal_score_snapshot();
                    let live_scores = sim.cooperative_score_rows();
                    ensure!(
                        statistics.rows.len() == self.expected_players,
                        "VMCE victory statistics have the wrong number of rows"
                    );
                    for row in &statistics.rows {
                        let assignment = assignments
                            .participants
                            .iter()
                            .find(|assignment| assignment.participant_name == row.player)
                            .context("VMCE victory player has no frozen assignment")?;
                        let values = terminal
                            .and_then(|snapshot| {
                                snapshot
                                    .rows
                                    .iter()
                                    .find(|score| {
                                        sim.interner().resolve(score.owner) == assignment.owner_name
                                    })
                                    .map(|score| {
                                        (
                                            score.kills,
                                            score.losses,
                                            score.built,
                                            score.score,
                                            score.credits_earned,
                                        )
                                    })
                            })
                            .or_else(|| {
                                live_scores
                                    .iter()
                                    .find(|score| {
                                        sim.interner().resolve(score.owner) == assignment.owner_name
                                    })
                                    .map(|score| {
                                        (
                                            score.kills,
                                            score.losses,
                                            score.built,
                                            score.score,
                                            score.credits_earned,
                                        )
                                    })
                            })
                            .context("VMCE victory House has no score")?;
                        ensure!(
                            row.owner == assignment.owner_name
                                && (row.kills, row.losses, row.built, row.score, row.income)
                                    == values,
                            "VMCE victory statistics for {} do not match House {}",
                            row.player,
                            assignment.owner_name
                        );
                    }
                }
                let vmce_defeat_page = defeat::page(state).is_some();
                if victory_page.is_none() && !vmce_defeat_page {
                    self.vera_outcome_phase = VeraOutcomePhase::NextMission;
                    return Ok(());
                }
                ensure!(
                    render_frame < opened.saturating_add(1_200),
                    "VMCE round did not become ready: host={}, continue_enabled={}",
                    self.host(),
                    state.match_state.session.vmce_result_continue_enabled()
                );
                if !state.match_state.session.vmce_result_continue_enabled() {
                    return Ok(());
                }
                // On the VMCE retry page the host's Retry record is only
                // accepted after every online guest voted; the button/projection
                // exposes that gate separately from Continue's enabled state.
                if matches!(outcome, VmceOutcome::Retry)
                    && self.host()
                    && !state
                        .match_state
                        .session
                        .vmce_result_retry_state()
                        .is_some_and(|(_, lit, enabled)| lit && enabled)
                {
                    return Ok(());
                }
                if victory_page.is_some() {
                    // Host Continue / guest Ready through the production
                    // `0x103` input handlers over VMCE's right-panel slot.
                    let page = state
                        .frontend
                        .campaign_score_page
                        .as_ref()
                        .context("VMCE victory 0x103 page")?;
                    let (width, height) =
                        crate::app::frontend::shell_layout::shell_screen_size(state);
                    let fills = crate::app::frontend::shell_layout::fills_window(state);
                    let layout = page.layout(width, height, fills);
                    let ready = crate::app::vmce_result::types::button_rects(layout.scale).ready;
                    set_window_pointer_to_rect_center(state, ready);
                    App::handle_campaign_score_mouse_move(state);
                    App::handle_campaign_score_mouse_down(state);
                    App::handle_campaign_score_mouse_up(state);
                    MoviesCapture::restore_neutral_pointer(state);
                    if !self.host() {
                        ensure!(
                            state
                                .frontend
                                .vmce_result_page
                                .as_ref()
                                .and_then(|page| page.victory())
                                .and_then(|page| page.board.as_ref())
                                .is_some_and(|board| board.local_ready),
                            "VMCE Ready click did not update the result page"
                        );
                    }
                } else {
                    ensure!(
                        vmce_defeat_page,
                        "VMCE result round has neither a victory nor a defeat page"
                    );
                    defeat::apply_action(state, VmceDefeatAction::Retry);
                }
                if let Some(evidence) = self.vera_outcome_evidence.as_mut()
                    && let Some(object) = evidence.as_object_mut()
                {
                    object.insert("round_continue_frame".to_owned(), json!(render_frame));
                }
                self.event(
                    render_frame,
                    if self.host() {
                        "VERA outcome: host Continue started the next stage"
                    } else {
                        "VERA outcome: guest ready sent from the result dialog"
                    },
                );
                self.vera_outcome_phase = match outcome {
                    VmceOutcome::End => VeraOutcomePhase::Ending,
                    VmceOutcome::Retry => VeraOutcomePhase::Retrying,
                    _ => VeraOutcomePhase::NextMission,
                };
            }
            VeraOutcomePhase::Retrying => {
                let committed = state
                    .match_state
                    .sim_runtime
                    .runtime()
                    .map(|runtime| runtime.simulation.session().binary_frame())
                    .unwrap_or(0);
                self.step_vera_outcome(state, committed)?;
            }
            VeraOutcomePhase::Ending => {
                if state.frontend.credits_roll.is_some() {
                    let started = *self.vera_outcome_next_frame.get_or_insert(render_frame);
                    if render_frame == started {
                        self.request_screenshot("vera-campaign-credits");
                    } else if render_frame == started.saturating_add(90) {
                        App::credits_roll_input(
                            state,
                            crate::app::frontend::credits_roll::CreditsInput::Escape,
                        );
                    }
                    return Ok(());
                }
                if self.vera_outcome_next_frame.is_some()
                    && state.frontend.campaign_score_page.is_none()
                    && state.frontend.fullscreen_movie.is_none()
                {
                    let entered = *self.pair_linger_wall.get_or_insert_with(Instant::now);
                    if entered.elapsed() < std::time::Duration::from_secs(3) {
                        return Ok(());
                    }
                    ensure!(
                        state.match_state.session.network_return_context().is_none()
                            && state.frontend.lan.is_none()
                            && state.frontend.shell_route
                                == crate::app::shell_route::ShellRoute::MainMenu,
                        "VMCE campaign completion returned to a network room"
                    );
                    let evidence = json!({
                        "kind": "end",
                        "completed": true,
                        "render_frame": render_frame,
                        "screen": format!("{:?}", state.frontend.screen),
                        "network_return": format!("{:?}", state.match_state.session.network_return_context()),
                    });
                    if let Some(receipt) = self.final_receipt.as_mut() {
                        receipt["vera_outcome"] = evidence.clone();
                    }
                    self.vera_outcome_evidence = Some(evidence);
                    self.request_screenshot("vera-campaign-end");
                    self.event(
                        render_frame,
                        "VERA outcome: campaign completion returned to the menu",
                    );
                    self.phase = Phase::Finished;
                } else {
                    ensure!(
                        render_frame < 20_000,
                        "VMCE campaign completion did not finish: screen={:?}",
                        state.frontend.screen
                    );
                }
            }
            VeraOutcomePhase::NextMission => {
                if state.frontend.shell_route.network_setup()
                    && state.match_state.session.vmce_intermission_map().is_some()
                    && !crate::app::frontend::shell_transition::shell_slide_running(state)
                    && let Some(view) = setup::view(state)
                {
                    let session = &state.match_state.session;
                    let launch = session
                        .network_setup()
                        .context("VMCE intermission roster")?;
                    let local = launch.local_player_index;
                    let ready = launch.humans.iter().enumerate().all(|(index, _)| {
                        index == local
                            || (session.network_player_ready(index)
                                && session.network_map_status(index) == Some(1))
                    });
                    let local_ready = session.network_player_ready(local);
                    let host_can_start = self.host()
                        && ready
                        && !view.start_requested
                        && view.enabled(crate::ui::multiplayer_setup::START);
                    let guest_can_accept = !self.host()
                        && !local_ready
                        && view.enabled(crate::ui::multiplayer_setup::ACCEPT);
                    if host_can_start || guest_can_accept {
                        if !self.vera_intermission_shot_done {
                            self.request_screenshot("vera-intermission-lobby");
                            self.vera_intermission_shot_done = true;
                            self.event(
                                render_frame,
                                "VMCE intermission lobby captured after role projection and map admission, before the next-stage action",
                            );
                            return Ok(());
                        }
                    }
                    if host_can_start {
                        setup_button(state, crate::ui::multiplayer_setup::START)?;
                        self.event(
                            render_frame,
                            "VMCE intermission: host started the next stage",
                        );
                    } else if guest_can_accept {
                        setup_button(state, crate::ui::multiplayer_setup::ACCEPT)?;
                        self.event(
                            render_frame,
                            "VMCE intermission: guest readied the next stage",
                        );
                    }
                }
                if state.frontend.screen == GameScreen::InGame
                    && result_page(state) == VmceResultPageKind::None
                {
                    let native_frame = state
                        .match_state
                        .sim_runtime
                        .runtime()
                        .map(|runtime| runtime.simulation.session().binary_frame());
                    let first = *self.vera_outcome_next_frame.get_or_insert(render_frame);
                    // Both peers must read the same frame; frame 60 is after
                    // the loading handoff has settled.
                    if native_frame == Some(60) {
                        let state_hash = state
                            .match_state
                            .sim_runtime
                            .runtime()
                            .map(|runtime| runtime.simulation.state_hash());
                        let map_file = state
                            .match_state
                            .session
                            .network_setup()
                            .and_then(|launch| launch.selected_map_file.clone());
                        self.request_screenshot("vera-next-mission");
                        if let Some(evidence) = self.vera_outcome_evidence.as_mut()
                            && let Some(object) = evidence.as_object_mut()
                        {
                            object.insert(
                                "next_mission".to_owned(),
                                json!({
                                    "map": map_file,
                                    "native_frame": native_frame,
                                    "state_hash": state_hash,
                                    "render_frame": render_frame,
                                }),
                            );
                        }
                        if let (Some(receipt), Some(evidence)) = (
                            self.final_receipt.as_mut(),
                            self.vera_outcome_evidence.as_ref(),
                        ) {
                            receipt["vera_outcome"] = evidence.clone();
                        }
                        self.event(
                            render_frame,
                            "VERA outcome: the next campaign stage is live",
                        );
                        self.phase = Phase::Finished;
                    } else {
                        ensure!(
                            render_frame.saturating_sub(first) < 600,
                            "VMCE next campaign stage did not reach native frame 60"
                        );
                    }
                } else {
                    ensure!(
                        render_frame
                            < self
                                .vera_outcome_dialog_frame
                                .unwrap_or(0)
                                .saturating_add(VERA_OUTCOME_REAL_TIMEOUT_FRAMES),
                        "VMCE next campaign stage did not load: render_frame={render_frame}"
                    );
                }
            }
            VeraOutcomePhase::Quit => {
                let opened = self
                    .vera_outcome_dialog_frame
                    .context("VERA result dialog frame")?;
                if render_frame >= opened.saturating_add(4) {
                    // Leave through VMCE's own result owner: the defeat page's
                    // Leave action, or the victory page's Leave Room record.
                    if defeat::page(state).is_some() {
                        defeat::apply_action(state, VmceDefeatAction::Leave);
                    } else {
                        ensure!(
                            crate::app::vmce_result::leave_room_pressed(state),
                            "VMCE victory page did not accept Leave Room"
                        );
                    }
                    if let Some(evidence) = self.vera_outcome_evidence.as_mut()
                        && let Some(object) = evidence.as_object_mut()
                    {
                        object.insert("quit_render_frame".to_owned(), json!(render_frame));
                    }
                    if let (Some(receipt), Some(evidence)) = (
                        self.final_receipt.as_mut(),
                        self.vera_outcome_evidence.as_ref(),
                    ) {
                        receipt["vera_outcome"] = evidence.clone();
                    }
                    self.event(
                        render_frame,
                        "VERA outcome: Quit sent from the VMCE result dialog",
                    );
                    self.phase = Phase::Finished;
                }
            }
            VeraOutcomePhase::Prefix | VeraOutcomePhase::Inject => {}
        }
        Ok(())
    }

    /// The VERA outcome diagnostic: keep the mission prefix evidence, inject
    /// one outcome through the same production owner a map action reaches,
    /// then observe the result dialog (or the continued mission). Both peers
    /// inject at the same absolute committed frame, so the shared world sees
    /// one deterministic mutation.
    pub(super) fn step_vera_outcome(&mut self, state: &mut AppState, committed: u32) -> Result<()> {
        use crate::sim::house_state::HouseOutcomeKind;

        let outcome = self.vera_outcome.context("VERA outcome diagnostic")?;
        // Real-trigger runs never inject: arm a receipt as soon as the mission
        // is live, then let `finish_vera_outcome` observe the map's own chain.
        if matches!(
            outcome,
            VmceOutcome::Real | VmceOutcome::End | VmceOutcome::Retry
        ) {
            // The receipt/exit validators consume the regular 300-frame
            // checksum/hash prefix, so arm once that prefix exists rather than
            // on an arbitrary early frame.
            // VMCE's own result page is the arm point; otherwise the
            // mission may stay live until the checksum prefix exists.
            let result_open = result_page(state) != VmceResultPageKind::None;
            if self.vera_outcome_phase == VeraOutcomePhase::Prefix
                && state.frontend.screen == GameScreen::InGame
                && (result_open
                    || (committed >= self.required_frames() && !self.simulation_hashes.is_empty()))
            {
                self.final_receipt = Some(self.vera_receipt(state)?);
                self.vera_outcome_phase = VeraOutcomePhase::Observe;
                self.vera_outcome_render_frames = 0;
                self.event(
                    committed,
                    "VERA outcome: real-trigger mode armed; waiting for the map's own outcome action",
                );
            }
            return Ok(());
        }
        match self.vera_outcome_phase {
            VeraOutcomePhase::Prefix => {
                if committed >= VERA_OUTCOME_MISSION_SHOT_FRAME {
                    let input_locked = crate::app::input::campaign_input::is_locked(state);
                    let movement_verified = self.stopped
                        && self.simple_inputs_applied()
                        && self.observed_movement
                        && self.stop_observation.position().is_some();
                    // Some authored maps deliberately lock tactical input through
                    // the whole early mission. Do not manufacture a Move there:
                    // keep the checksum prefix, and record the explicit reason.
                    // If input was already attempted, it must have completed.
                    let locked_without_input =
                        input_locked && self.actor.is_none() && self.actions.is_empty();
                    ensure!(
                        self.checksums.len() >= self.required_frames() as usize
                            && !self.simulation_hashes.is_empty()
                            && (movement_verified || locked_without_input),
                        "VERA outcome prefix was incomplete before its screenshot: frame={committed}, checksums={}/{}, input_locked={}, stopped={}, inputs={}, moved={}, stop_settled={}",
                        self.checksums.len(),
                        self.required_frames(),
                        input_locked,
                        self.stopped,
                        self.simple_inputs_applied(),
                        self.observed_movement,
                        self.stop_observation.position().is_some()
                    );
                    self.request_screenshot("vera-mission");
                    // Build the receipt while the world and its roster are still
                    // resident; the outcome run retires the match before exiting.
                    let mut receipt = self.vera_receipt(state)?;
                    receipt["outcome_prefix"] = json!({
                        "input_locked": input_locked,
                        "movement_verified": movement_verified,
                        "checksum_count": self.checksums.len(),
                    });
                    if locked_without_input {
                        receipt["scope"] = json!(
                            "VMCE LAN result diagnostic: real room setup and a 300-frame checksum prefix; authored input lock prevented Move/Stop, which were not validated"
                        );
                    }
                    self.final_receipt = Some(receipt);
                    self.vera_outcome_phase = VeraOutcomePhase::Inject;
                    self.event(
                        committed,
                        "VERA outcome: prefix complete; mission screenshot queued",
                    );
                }
            }
            VeraOutcomePhase::Inject => {
                if committed >= VERA_OUTCOME_INJECT_FRAME {
                    let runtime = state
                        .match_state
                        .sim_runtime
                        .write_sim(SimWrite::Debug)
                        .context("live simulation")?;
                    let actual_frame = runtime.simulation.session().binary_frame();
                    ensure!(
                        actual_frame == VERA_OUTCOME_INJECT_FRAME,
                        "VMCE outcome injection missed the shared committed frame: observed={committed}, actual={actual_frame}, expected={VERA_OUTCOME_INJECT_FRAME}"
                    );
                    ensure!(
                        !runtime.simulation.pending_network_wait(),
                        "VMCE outcome injection reached frame {actual_frame} during an unfinished network tick"
                    );
                    // Direct field projections keep the rules borrow disjoint
                    // from the mutable simulation borrow.
                    let rules = &runtime.resources.rules;
                    match outcome {
                        VmceOutcome::Win => {
                            runtime.simulation.dev_trigger_current_house_outcome(
                                &rules,
                                HouseOutcomeKind::Victory,
                            );
                        }
                        VmceOutcome::Lose => {
                            runtime.simulation.dev_trigger_current_house_outcome(
                                &rules,
                                HouseOutcomeKind::Defeat,
                            );
                        }
                        VmceOutcome::HouseLoss => {
                            let owners = runtime.simulation.player_outcome_viewpoints();
                            let marked: Vec<String> = owners
                                .iter()
                                .map(|owner| {
                                    runtime.simulation.interner().resolve(*owner).to_owned()
                                })
                                .collect();
                            for owner in owners {
                                runtime.simulation.dev_force_house_defeat(owner, &rules);
                            }
                            self.vera_outcome_houses = Some(json!(marked));
                        }
                        VmceOutcome::Real | VmceOutcome::End | VmceOutcome::Retry => {
                            unreachable!("real-trigger mode never injects an outcome")
                        }
                    }
                    self.vera_outcome_injected_frame = Some(actual_frame);
                    self.vera_outcome_phase = VeraOutcomePhase::Observe;
                    self.vera_outcome_render_frames = 0;
                    self.event(
                        committed,
                        match outcome {
                            VmceOutcome::Win => "VERA outcome: team victory injected",
                            VmceOutcome::Lose => "VERA outcome: team defeat injected",
                            VmceOutcome::HouseLoss => {
                                "VERA outcome: every player House marked defeated"
                            }
                            VmceOutcome::Real | VmceOutcome::End | VmceOutcome::Retry => {
                                "VERA outcome: real-trigger mode has no injected outcome"
                            }
                        },
                    );
                }
            }
            VeraOutcomePhase::Observe
            | VeraOutcomePhase::Round
            | VeraOutcomePhase::NextMission
            | VeraOutcomePhase::Ending
            | VeraOutcomePhase::Retrying
            | VeraOutcomePhase::Quit => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod result_page_tests {
    use super::{VmceResultPageKind, VmceResultPageView, vmce_duration_header};

    fn view(kind: VmceResultPageKind) -> VmceResultPageView {
        match kind {
            VmceResultPageKind::Victory => VmceResultPageView {
                campaign_score_page: true,
                victory_board: true,
                victory_statistics: true,
                result_round: true,
                defeat_page: false,
                defeat_statistics: false,
            },
            VmceResultPageKind::Defeat => VmceResultPageView {
                campaign_score_page: false,
                victory_board: false,
                victory_statistics: false,
                result_round: true,
                defeat_page: true,
                defeat_statistics: true,
            },
            VmceResultPageKind::None => VmceResultPageView::default(),
        }
    }

    #[test]
    fn a_won_mission_is_detected_through_the_vmce_0x103_page() {
        assert_eq!(
            view(VmceResultPageKind::Victory).kind(),
            VmceResultPageKind::Victory
        );
        // Detection needs the VMCE content and round, not the stock page alone.
        let mut incomplete = view(VmceResultPageKind::Victory);
        incomplete.victory_board = false;
        assert_eq!(incomplete.kind(), VmceResultPageKind::None);
        incomplete = view(VmceResultPageKind::Victory);
        incomplete.result_round = false;
        assert_eq!(incomplete.kind(), VmceResultPageKind::None);
    }

    #[test]
    fn a_lost_mission_is_detected_through_the_vmce_defeat_page() {
        assert_eq!(
            view(VmceResultPageKind::Defeat).kind(),
            VmceResultPageKind::Defeat
        );
        // The container alone is not enough: the statistics table is the
        // content the defeat detector waits for.
        let mut empty = view(VmceResultPageKind::Defeat);
        empty.defeat_statistics = false;
        assert_eq!(empty.kind(), VmceResultPageKind::None);
    }

    #[test]
    fn a_stock_result_page_is_not_a_vmce_result_page() {
        // A stock BC6 page or a bare stock campaign score page
        // sets none of VMCE's content slots, so win/lose never wait for it.
        assert_eq!(
            VmceResultPageView::default().kind(),
            VmceResultPageKind::None
        );
        let bare_stock_campaign_0x103 = VmceResultPageView {
            campaign_score_page: true,
            result_round: false,
            ..VmceResultPageView::default()
        };
        assert_eq!(bare_stock_campaign_0x103.kind(), VmceResultPageKind::None);
    }

    #[test]
    fn the_outcome_arm_predicate_only_accepts_vmce_pages() {
        // `step_vera_outcome`'s real-trigger arm uses `result_page(state)`. A
        // stock Cooperative BC6 page (or any page VMCE does not own) leaves
        // every VMCE content slot unset and must not arm the observation.
        let stock_bc6_only = VmceResultPageView {
            campaign_score_page: false,
            victory_board: false,
            victory_statistics: false,
            result_round: false,
            defeat_page: false,
            defeat_statistics: false,
        };
        assert_eq!(stock_bc6_only.kind(), VmceResultPageKind::None);
        assert_ne!(
            view(VmceResultPageKind::Victory).kind(),
            VmceResultPageKind::None
        );
        assert_ne!(
            view(VmceResultPageKind::Defeat).kind(),
            VmceResultPageKind::None
        );
    }

    #[test]
    fn defeat_time_headers_are_the_bare_native_shape() {
        assert!(vmce_duration_header("00:00:13"));
        assert!(!vmce_duration_header(""));
        assert!(!vmce_duration_header("Time: 00:00:13"));
    }
}
