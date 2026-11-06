//! Two independent App processes, using the ordinary LAN service and frame
//! boundary. Only user actions are automated. No peer result, loading report,
//! synchronized record, checksum or simulation step is injected here.

use super::*;

/// Navigate with the normal buttons before resolving a resource control. Its
/// visible slot is never the participant's absolute row on an extended roster.
/// `row`'s control on the roster pages, paged to through normal input. A
/// VERA room lists only who is in it, so a row's place there is not its
/// index; the search starts from the first page.
fn setup_control_at_row(
    state: &mut AppState,
    column: SetupColumn,
    row: usize,
) -> Result<Option<u16>> {
    use crate::ui::multiplayer_setup::{NEXT_ROWS, PREVIOUS_ROWS, VISIBLE_SETUP_ROWS};
    let pages = crate::skirmish_launch::NETWORK_PLAYER_SLOT_COUNT / VISIBLE_SETUP_ROWS + 1;
    for _ in 0..pages {
        if setup::view(state).context("setup view")?.first_row == 0 {
            break;
        }
        setup_button(state, PREVIOUS_ROWS)?;
    }
    for _ in 0..pages {
        let view = setup::view(state).context("setup view")?;
        if let Some(combo) = view.combos.iter().find(|combo| {
            combo.row as usize == row
                && combo.visible
                && SetupColumn::decode(combo.id).is_some_and(|(candidate, _)| candidate == column)
        }) {
            return Ok(Some(combo.id));
        }
        if view.first_row + view.dialog.rows_per_page() >= view.row_count {
            return Ok(None);
        }
        setup_button(state, NEXT_ROWS)?;
    }
    anyhow::bail!("setup page navigation did not reach the participant")
}
use crate::app::diagnostics::shell_capture::{LiveLanConfig, LiveLanTransport, VeraCoopOutcome};
use crate::net::udp::UdpBindConfig;
use crate::ui::multiplayer_setup::{SetupAction, SetupColumn};
use std::collections::{BTreeMap, BTreeSet};

mod battle;
mod cooperative;
mod failure;
mod latency;
mod mass;
mod online;
mod profile;
mod setup_crash;

const HOST_NAME: &str = "LiveHost";
const GUEST_NAME: &str = "LiveGuest";
const REQUIRED_FRAMES: u32 = 300;
const SOLO_AI_INPUT_BOUND_FRAMES: u32 = 1;
const VERA_COOP_SETUP_WATCHDOG: Duration = Duration::from_secs(60);
/// Render frames an accepted VERA Coop Start may spend before leaving the
/// setup shell for loading. A refusal keeps the host on MainMenu, so this
/// bounded window reports the refusal instead of the generic capture timeout.
const VERA_COOP_START_WATCHDOG_FRAMES: u32 = 600;
/// A VERA Coop peer that finished its own checks first keeps its transport
/// alive while the slower peer settles, so neither fails on a peer that
/// already exited. Both wait on each other; the wall clock bounds the wait.
const VERA_COOP_PAIR_LINGER_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// Diagnostic evidence only: a fixed local frame cannot prove that Stop has
/// arrived from the relay. Retain the full Position after actual execution,
/// cleared navigation and the same ten-frame quiet window as the mass capture.
#[derive(Default)]
struct StopObservation {
    quiet: Option<(u32, Value)>,
    settled: Option<(u32, Value)>,
}

impl StopObservation {
    fn observe(&mut self, frame: u32, applied: bool, position: Value, navigation_clear: bool) {
        if self.settled.is_some() {
            return;
        }
        if !applied || !navigation_clear {
            self.quiet = None;
            return;
        }
        match self.quiet.as_ref() {
            Some((since, previous)) if previous == &position => {
                if frame.saturating_sub(*since) >= 10 {
                    self.settled = Some((frame, position));
                }
            }
            _ => self.quiet = Some((frame, position)),
        }
    }

    fn position(&self) -> Option<&Value> {
        self.settled.as_ref().map(|(_, position)| position)
    }
}

#[cfg(test)]
mod stop_observation_tests {
    use super::{AiActivity, LanTarget, LiveLanCapture, StopObservation};
    use crate::app::match_diagnostics::CommandExecutionObservation;
    use crate::sim::command::{CommandRecord, IDLE_OPCODE, StopRecord};
    use serde_json::json;

    #[test]
    fn waits_for_applied_stop_and_quiet_position_beyond_old_frame_160() {
        let before = json!({"rx":10,"ry":10,"sub_x":128,"sub_y":128});
        let stopped = json!({"rx":12,"ry":10,"sub_x":128,"sub_y":128});
        let mut observation = StopObservation::default();
        observation.observe(160, false, before.clone(), true);
        assert_eq!(observation.position(), None);
        observation.observe(220, true, before.clone(), false);
        assert_eq!(observation.position(), None);
        observation.observe(221, true, before.clone(), true);
        observation.observe(225, true, stopped.clone(), true);
        observation.observe(234, true, stopped.clone(), true);
        assert_eq!(observation.position(), None);
        observation.observe(235, true, stopped.clone(), true);
        assert_eq!(observation.position(), Some(&stopped));
        observation.observe(600, true, before.clone(), true);
        assert_eq!(
            observation.position(),
            Some(&stopped),
            "retain the settled baseline so later movement is detected"
        );
    }

    #[test]
    fn delayed_online_execution_is_recorded_without_relaxing_solo_ai_deadline() {
        fn capture() -> LiveLanCapture {
            let mut capture = LiveLanCapture::new_config(LanTarget::OnlineHost, None, None);
            capture.actor = Some(7);
            capture
                .actions
                .push(json!({"input":"Stop","native_frame":80,"actor":7}));
            capture
        }
        fn execution(frame: u32) -> CommandExecutionObservation {
            let mut record = CommandRecord::encode(IDLE_OPCODE, 0, frame as i32, &[]).unwrap();
            StopRecord {
                house_id: 0,
                frame: frame as i32,
                entity_id: 7,
            }
            .write_into(&mut record);
            CommandExecutionObservation {
                observed_at: crate::app::match_diagnostics::ObservationTime::now(),
                frame,
                record,
                applied: true,
                actor: Some(7),
                motion: None,
                timing: None,
                destination: None,
                cell: Some((12, 10)),
                mission: Some((0, 0)),
            }
        }
        let mut online = capture();
        online
            .observe_simple_input_application(221, execution(220))
            .unwrap();
        assert_eq!(online.actions[0]["application_frame"], 220);
        assert_eq!(online.actions[0]["delay_frames"], 140);

        let mut solo = capture();
        solo.ai = Some(AiActivity::default());
        assert!(
            solo.observe_simple_input_application(221, execution(220))
                .is_err()
        );
        solo.observe_simple_input_application(82, execution(81))
            .unwrap();
        assert_eq!(solo.actions[0]["application_frame"], 81);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Main,
    Lobby,
    Discover,
    HostSetup,
    HostReady,
    GuestSetup,
    Loading,
    Running,
    Finished,
}

/// Read-only per-round evidence from the real AI House and production owners.
/// Reset with the capture round; it never drives AI or advances simulation.
#[derive(Default)]
struct AiActivity {
    owner: Option<String>,
    first_frame: u32,
    initial_entities: BTreeSet<u64>,
    factory_progress: BTreeMap<u64, u16>,
    activity: Option<Value>,
}

impl AiActivity {
    fn snapshot(&self, state: &AppState) -> Result<Value> {
        let runtime = state
            .match_state
            .sim_runtime
            .as_ref()
            .context("AI diagnostic simulation")?;
        let sim = &runtime.simulation;
        let owner = self
            .owner
            .as_deref()
            .and_then(|name| sim.interner().get(name))
            .context("AI diagnostic owner")?;
        let house = sim.houses().get(&owner).context("AI diagnostic House")?;
        let saved_house = serde_json::to_value(house)?;
        let entities = sim.entities().values_sorted().filter(|entity| entity.owner() == owner)
            .map(|entity| {
                let saved_entity = serde_json::to_value(entity)?;
                Ok(json!({"id":entity.stable_id(),"type":sim.interner().resolve(entity.type_ref()),
                    "alive":entity.is_alive(),"limbo":entity.in_limbo(),"health":entity.health.current(),
                    "position":entity.position,"mission":entity.mission.current().raw(),
                    "queued_mission":entity.mission.queued().raw(),"handler_state":entity.mission.handler_state(),
                    "dispatch_timer":entity.mission.dispatch_timer(),
                    "destination":entity.navigation.nav_com(),"pending_deploy":saved_entity.get("mcv_deploy_pending"),
                    "logic_registered":sim.logic_order().contains(&entity.stable_id())}))
            }).collect::<Result<Vec<Value>>>()?;
        Ok(
            json!({"native_frame":sim.session().binary_frame(),"first_native_frame":self.first_frame,
            "initial_entities":self.initial_entities,"activity":self.activity,
            "house_index":sim.houses().index_of(owner),"current_iq":house.current_iq(),
            "iq_production":runtime.resources.rules.general.iq_production,
            "game_mode_nonzero":sim.session().game_mode_nonzero(),"credits":house.economy().credits(),
            "is_defeated":house.is_defeated(),
            "bases":state.match_state.session.network_setup().map(|launch|launch.options.bases),
            "strategy_timer":house.strategy_timer(),
            "activation":saved_house.get("ai_activation"),"base_center":house.base_center(),
            "alternate_base_center":house.alternate_base_center(),
            "base_nodes":house.base_plan().nodes(),"construction_yards":house.build_const_order(),
            "production":house.ai_production(),"entities":entities}),
        )
    }

    fn observe(&mut self, state: &AppState) -> Result<()> {
        let launch = state
            .match_state
            .session
            .network_setup()
            .context("AI launch roster")?;
        ensure!(
            launch.humans.len() == 1 && launch.opponents.len() == 1,
            "AI smoke requires exactly one human and one explicit AI row"
        );
        let slots = crate::sim::scenario_bootstrap::normalized_launch_slots(launch);
        let ai = slots
            .iter()
            .filter(|slot| !slot.is_human && !slot.observer)
            .collect::<Vec<_>>();
        ensure!(
            ai.len() == 1 && ai[0].source_human_index().is_none(),
            "AI acquired a human source"
        );
        let sim = &state
            .match_state
            .sim_runtime
            .as_ref()
            .context("AI live simulation")?
            .simulation;
        let owner = sim
            .interner()
            .get(&ai[0].owner_name)
            .context("normalized AI owner")?;
        let house = sim.houses().get(&owner).context("AI House")?;
        ensure!(
            !house.is_human() && !house.player_control() && !house.multiplay_passive(),
            "selected CPU is not an active computer House"
        );
        ensure!(
            sim.entities()
                .values_sorted()
                .filter(|entity| entity.is_alive()
                    && !entity.in_limbo()
                    && matches!(
                        entity.category(),
                        crate::map::entities::EntityCategory::Infantry
                            | crate::map::entities::EntityCategory::Unit
                            | crate::map::entities::EntityCategory::Aircraft
                    ))
                .count()
                <= 1000,
            "AI smoke exceeded its 1000-unit observation limit"
        );
        let frame = sim.session().binary_frame();
        let initialized = self.owner.is_some();
        if let Some(expected) = &self.owner {
            ensure!(
                expected == &ai[0].owner_name,
                "AI owner changed during the live round"
            );
        } else {
            self.owner = Some(ai[0].owner_name.clone());
            self.first_frame = frame;
            self.initial_entities = sim
                .entities()
                .values_sorted()
                .filter(|entity| entity.owner() == owner && entity.is_alive() && !entity.in_limbo())
                .map(|entity| entity.stable_id())
                .collect();
        }
        for factory in sim.production().factories().iter_insertion_ordered() {
            if factory.owner() != owner {
                continue;
            }
            let progress = factory.progress();
            let previous = self
                .factory_progress
                .insert(factory.insertion_seq(), progress);
            if self.activity.is_none()
                && let Some(previous) = previous.filter(|previous| progress > *previous)
            {
                self.activity = Some(json!({"kind":"factory_progress","native_frame":frame,
                    "factory":factory.insertion_seq(),"from":previous,"to":progress,
                    "type":factory.object().map(|object|sim.interner().resolve(object.type_id))}));
            }
        }
        if initialized
            && self.activity.is_none()
            && let Some(entity) = sim.entities().values_sorted().find(|entity| {
                entity.owner() == owner
                    && entity.is_alive()
                    && !entity.in_limbo()
                    && !self.initial_entities.contains(&entity.stable_id())
            })
        {
            self.activity = Some(json!({"kind":"new_ai_entity","native_frame":frame,
                "entity":entity.stable_id(),"type":sim.interner().resolve(entity.type_ref()),
                "category":format!("{:?}",entity.category())}));
        }
        Ok(())
    }

    fn receipt(&self) -> Result<Value> {
        ensure!(
            self.owner.is_some() && self.activity.is_some(),
            "AI activity was not observed"
        );
        Ok(
            json!({"owner":self.owner,"is_human":false,"player_control":false,"multiplay_passive":false,
            "first_native_frame":self.first_frame,"initial_live_entities":self.initial_entities.len(),
            "activity":self.activity}),
        )
    }
}

pub(super) struct LiveLanCapture {
    target: LanTarget,
    config: Option<LiveLanConfig>,
    rounds: Option<online::LiveRoundFlow>,
    frame_budget: Option<u32>,
    expected_players: usize,
    phase: Phase,
    route: Vec<Value>,
    /// Observations of the existing checksum ring, indexed by native frame.
    checksums: Vec<u32>,
    /// The existing replay owner's shared-world hashes for the observed prefix.
    /// Retained across teardown because a connection Quit flushes that log.
    simulation_hashes: Vec<(u64, u64)>,
    startup: Option<Value>,
    /// Read-only proof retained before the setup protocol is dropped for loading.
    map_admission: Option<Value>,
    actions: Vec<Value>,
    actor: Option<u64>,
    actor_last_seen: Option<Value>,
    origin: Option<(u16, u16)>,
    observed_movement: bool,
    stop_observation: StopObservation,
    stopped: bool,
    last_wait: Option<String>,
    /// The guest watched from a spectator row (`switch_sides`).
    guest_watched: bool,
    /// The guest's Side list steps: Observer asked, a side asked, playing.
    sides: u8,
    cooperative: Option<cooperative::CooperativeRun>,
    country_selected: bool,
    battle: Option<battle::BattleOpening>,
    mass: Option<mass::MassInput>,
    latency: Option<latency::LatencyCapture>,
    ai: Option<AiActivity>,
    failure: Option<failure::FailureCapture>,
    setup_crash: Option<setup_crash::SetupCrashCapture>,
    coop_setup_initial: Option<Value>,
    coop_setup_final: Option<Value>,
    coop_setup_guest_accepted: bool,
    coop_setup_started: Option<Instant>,
    /// The render frame the VERA Coop host clicked the production Start.
    coop_setup_started_frame: Option<u32>,
    /// First committed frame whose authored map locked tactical input.
    locked_frames: Option<u32>,
    /// First committed frame whose VERA Coop pair conditions were met.
    pair_linger_at: Option<u32>,
    pair_linger_wall: Option<Instant>,
    /// The peer-House control probe, recorded once after GO.
    vera_probe: Option<Value>,
    /// `--vera-outcome`: the mission outcome to inject after the prefix.
    vera_outcome: Option<VeraCoopOutcome>,
    vera_outcome_phase: VeraOutcomePhase,
    vera_outcome_evidence: Option<Value>,
    vera_outcome_injected_frame: Option<u32>,
    vera_outcome_dialog_frame: Option<u32>,
    /// Render frames observed since the injection (the shared sim may be done).
    vera_outcome_render_frames: u32,
    /// House names the houses-mode injection marked defeated.
    vera_outcome_houses: Option<Value>,
    /// Receipt built while an outcome run's world and roster were resident.
    final_receipt: Option<Value>,
    /// Checkpoint screenshot labels this capture wants written.
    screenshot_requests: Vec<String>,
}

/// The VERA outcome diagnostic's own small state machine. Every step reads the
/// shared committed frame, so both peers inject at the same native frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VeraOutcomePhase {
    /// Wait for the mission prefix, then ask for its screenshot.
    Prefix,
    /// The prefix screenshot is queued; inject at the fixed frame.
    Inject,
    /// Win/lose: wait for BC6. Houses: wait out the continued-mission window.
    Observe,
    /// The BC6 page is up; quit after a short hold.
    Quit,
}

/// Both peers use the same absolute frame so the injected outcome enters the
/// shared world at the same native frame, not at a per-process readiness edge.
const VERA_OUTCOME_MISSION_SHOT_FRAME: u32 = 1_100;
const VERA_OUTCOME_INJECT_FRAME: u32 = 1_400;
/// Render frames the continued mission must stay alive after a House loss.
const VERA_OUTCOME_CONTINUED_FRAMES: u32 = 150;

impl LiveLanCapture {
    pub(super) fn new(target: LanTarget, config: LiveLanConfig) -> Self {
        let flow = matches!(
            target,
            LanTarget::LiveHost
                | LanTarget::LiveGuest
                | LanTarget::LiveDesyncHost
                | LanTarget::LiveDesyncGuest
        )
        .then(online::LiveRoundFlow::new_lan);
        Self::new_config(target, Some(config), flow)
    }

    pub(super) fn new_online(
        target: LanTarget,
        config: crate::app::diagnostics::shell_capture::LiveOnlineConfig,
    ) -> Self {
        let startup_failure = config
            .startup_host_loss
            .as_ref()
            .map(|_| failure::FailureCapture::startup(config.clone()));
        let frames = config.frames;
        let players = config.players;
        let simple = config.simple_input;
        let mass = config.mass_input;
        let latency = config.latency;
        let ai = config.ai.is_some();
        let mut capture = Self::new_config(target, None, Some(online::LiveRoundFlow::new(config)));
        if let Some(failure) = startup_failure {
            capture.failure = Some(failure);
        }
        capture.frame_budget = Some(frames);
        capture.expected_players = players;
        capture.ai = ai.then(AiActivity::default);
        if simple {
            capture.battle = None;
        }
        if latency {
            capture.latency = Some(latency::LatencyCapture::default());
        }
        if mass {
            capture.mass = Some(mass::MassInput::default());
        }
        capture
    }

    fn new_config(
        target: LanTarget,
        config: Option<LiveLanConfig>,
        rounds: Option<online::LiveRoundFlow>,
    ) -> Self {
        let setup_crash = config
            .as_ref()
            .and_then(|config| config.setup_crash)
            .map(setup_crash::SetupCrashCapture::new);
        let vera_outcome = config.as_ref().and_then(|config| config.vera_outcome);
        Self {
            target,
            config,
            rounds,
            setup_crash,
            frame_budget: None,
            expected_players: 2,
            phase: Phase::Main,
            route: Vec::new(),
            checksums: Vec::new(),
            simulation_hashes: Vec::new(),
            startup: None,
            map_admission: None,
            actions: Vec::new(),
            actor: None,
            actor_last_seen: None,
            origin: None,
            observed_movement: false,
            stop_observation: StopObservation::default(),
            stopped: false,
            last_wait: None,
            coop_setup_initial: None,
            coop_setup_final: None,
            coop_setup_guest_accepted: false,
            coop_setup_started: None,
            coop_setup_started_frame: None,
            locked_frames: None,
            pair_linger_at: None,
            pair_linger_wall: None,
            vera_probe: None,
            vera_outcome,
            vera_outcome_phase: VeraOutcomePhase::Prefix,
            vera_outcome_evidence: None,
            vera_outcome_injected_frame: None,
            vera_outcome_dialog_frame: None,
            vera_outcome_render_frames: 0,
            vera_outcome_houses: None,
            final_receipt: None,
            screenshot_requests: Vec::new(),
            guest_watched: false,
            sides: 0,
            cooperative: target.live_cooperative().then(|| {
                cooperative::CooperativeRun::new(
                    matches!(
                        target,
                        LanTarget::LiveCoopSavedHost | LanTarget::LiveCoopFinalHost
                    ),
                    match target {
                        LanTarget::LiveCoopFinalHost => cooperative::CoopEnding::CampaignEnd {
                            continue_to_shell: false,
                        },
                        LanTarget::LiveCoopFinalGuest => cooperative::CoopEnding::CampaignEnd {
                            continue_to_shell: true,
                        },
                        LanTarget::OnlineCoopHost | LanTarget::OnlineCoopGuest => {
                            cooperative::CoopEnding::Advance
                        }
                        _ => cooperative::CoopEnding::Retry,
                    },
                    target.online(),
                )
            }),
            country_selected: false,
            battle: matches!(
                target,
                LanTarget::BattleHost
                    | LanTarget::BattleGuest
                    | LanTarget::OnlineHost
                    | LanTarget::OnlineGuest
            )
            .then(|| battle::BattleOpening::new(!target.online())),
            mass: None,
            latency: None,
            ai: None,
            failure: match target {
                LanTarget::LiveTimeoutHost | LanTarget::OnlineTimeoutHost => {
                    Some(failure::FailureCapture::new(failure::Mode::Timeout))
                }
                LanTarget::LiveQuitHost | LanTarget::OnlineQuitHost => {
                    Some(failure::FailureCapture::new(failure::Mode::Quit))
                }
                LanTarget::LiveDesyncHost | LanTarget::LiveDesyncGuest => {
                    Some(failure::FailureCapture::new(failure::Mode::Desync))
                }
                _ => None,
            },
        }
    }

    fn host(&self) -> bool {
        matches!(
            self.target,
            LanTarget::OnlineHost
                | LanTarget::OnlineTimeoutHost
                | LanTarget::OnlineQuitHost
                | LanTarget::OnlineCoopHost
                | LanTarget::LiveHost
                | LanTarget::LiveTimeoutHost
                | LanTarget::LiveQuitHost
                | LanTarget::LiveDesyncHost
                | LanTarget::LiveCoopHost
                | LanTarget::LiveCoopSavedHost
                | LanTarget::LiveCoopFinalHost
                | LanTarget::LiveVeraCoopHost
                | LanTarget::BattleHost
        )
    }

    fn required_frames(&self) -> u32 {
        if let Some(frames) = self.frame_budget {
            return frames;
        }
        if self.battle.is_some() {
            // Includes paid GI rally/combat, building repair and sale with an
            // unfinished Factory queue, natural crew and cleanup. Every frame
            // still belongs to normal production pacing/pathing/combat.
            7200
        } else {
            REQUIRED_FRAMES
        }
    }

    fn select_country(&mut self, state: &mut AppState) -> Result<()> {
        if self.battle.is_none() || self.country_selected {
            return Ok(());
        }
        let country = state
            .process_assets
            .multiplayer_countries()
            .position(|(id, _)| id.eq_ignore_ascii_case("Americans"))
            .context("retail Americans country")?;
        let row = state
            .match_state
            .session
            .network_setup()
            .map(|launch| launch.local_player())
            .and_then(|human| human.raw_slot_index)
            .context("local setup row")?;
        let control = setup_control_at_row(state, SetupColumn::Country, usize::from(row))?
            .context("visible local country row")?;
        setup::apply_action(
            state,
            SetupAction::Select {
                control,
                value: i32::try_from(country)?,
            },
        );
        self.country_selected = true;
        Ok(())
    }

    /// Initial joined setup has neither host metadata nor a canonical roster.
    /// Wait for that admission, including the intended player capacity, before
    /// treating the local combo's available waypoints as authoritative.
    fn start_selection_admitted(&self, state: &AppState) -> bool {
        let session = &state.match_state.session;
        (session.host() || session.network_start_seed().is_some())
            && session
                .network_map_metadata()
                .is_some_and(|metadata| metadata.player_capacity >= self.expected_players as i32)
            && session.network_setup().is_some_and(|launch| {
                launch.selected_map_file.is_some()
                    && launch.humans.len() == self.expected_players
                    && launch
                        .humans
                        .iter()
                        .all(|human| human.raw_slot_index.is_some())
            })
    }

    fn start_matches(&self, state: &AppState, start: Option<u8>) -> bool {
        start.is_none_or(|start| {
            state
                .match_state
                .session
                .network_setup()
                .is_some_and(|launch| {
                    launch.local_player().slot.start_position
                        == crate::skirmish_launch::LaunchStartPosition::Position(start)
                })
        })
    }

    /// The host paired with `lan-live-guest` starts only once it watched
    /// and plays again.
    fn waits_for_spectator(&self) -> bool {
        self.target == LanTarget::LiveHost
    }

    /// In setup the guest picks Observer in its Side list: VERA's room
    /// moves it to the open spectator row. Then it picks a side, and the
    /// room seats it again.
    fn switch_sides(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        if self.target != LanTarget::LiveGuest {
            return Ok(());
        }
        let Some(local) = state
            .match_state
            .session
            .network_setup()
            .map(|launch| launch.local_player().clone())
        else {
            return Ok(());
        };
        let Some(row) = local.raw_slot_index else {
            return Ok(());
        };
        match (self.sides, local.is_observer()) {
            (0, false) | (1, true) => {}
            (2, false) => {
                self.sides = 3;
                self.event(frame, &format!("guest plays again from row {row}"));
                return Ok(());
            }
            _ => return Ok(()),
        }
        let control = setup_control_at_row(state, SetupColumn::Country, usize::from(row))?
            .context("visible local Side list")?;
        let view = setup::view(state).context("Side list view")?;
        let value = view
            .combo(control)
            .context("local Side list")?
            .choices
            .iter()
            .map(|item| item.value)
            .find(|value| (*value == -3) != local.is_observer())
            .filter(|_| view.enabled(control));
        let Some(value) = value else {
            return Ok(());
        };
        setup::apply_action(state, SetupAction::Select { control, value });
        self.sides += 1;
        self.event(
            frame,
            &if local.is_observer() {
                format!("guest watches from row {row}; picks side {value}")
            } else {
                format!("guest picks Observer from row {row}")
            },
        );
        Ok(())
    }

    /// A guest's game accepts by itself, so readiness no longer says the guest
    /// has made its choices. The host's Start also waits for the choices this
    /// run makes: battle runs pick Americans for every player, and a run that
    /// fixes starts gives every process its own `--online-start`.
    fn choices_made(&self, state: &AppState, fixed_starts: bool) -> bool {
        state
            .match_state
            .session
            .network_setup()
            .is_some_and(|launch| {
                launch.humans.iter().all(|human| {
                    (self.battle.is_none() || !human.slot.country_random)
                        && (!fixed_starts
                            || matches!(
                                human.slot.start_position,
                                crate::skirmish_launch::LaunchStartPosition::Position(_)
                            ))
                })
            })
    }

    /// Select an authored waypoint through the local human's visible Start
    /// combo. The configured waypoint is never used as a source/raw-row index.
    /// Read the real choice each time: later map selection can reset it to Auto.
    fn select_start(&mut self, state: &mut AppState, start: Option<u8>) -> Result<bool> {
        let Some(start) = start else {
            return Ok(false);
        };
        if self.start_matches(state, Some(start)) || !self.start_selection_admitted(state) {
            return Ok(false);
        }
        let row = state
            .match_state
            .session
            .network_setup()
            .and_then(|launch| launch.local_player().raw_slot_index)
            .context("local start-choice row")?;
        let control = setup_control_at_row(state, SetupColumn::Start, usize::from(row))?
            .context("visible local Start combo")?;
        let view = setup::view(state).context("start-choice view")?;
        let combo = view.combo(control).context("local Start combo")?;
        ensure!(
            view.enabled(control)
                && combo
                    .choices
                    .iter()
                    .any(|choice| choice.value == i32::from(start)),
            "configured start is not available through the normal local combo"
        );
        setup::apply_action(
            state,
            SetupAction::Select {
                control,
                value: i32::from(start),
            },
        );
        ensure!(
            state
                .match_state
                .session
                .network_setup()
                .is_some_and(|launch| launch.local_player().slot.start_position
                    == crate::skirmish_launch::LaunchStartPosition::Position(start)),
            "normal Start control did not select the configured waypoint"
        );
        Ok(true)
    }

    fn event(&mut self, frame: u32, action: &str) {
        log::debug!("live LAN {:?} render {frame}: {action}", self.target);
        self.route
            .push(json!({"render_frame":frame,"action":action}));
    }

    pub(super) fn ready(&self) -> bool {
        if let Some(crash) = &self.setup_crash {
            return crash.ready();
        }
        self.rounds
            .as_ref()
            .map_or(self.phase == Phase::Finished, online::LiveRoundFlow::ready)
    }

    /// The battle observer checks each command at its execution boundary and
    /// repair on consecutive frames. Ordinary pacing runs every due frame in
    /// one pass, for example the next frame right after a network wait ends,
    /// so the battle asks for one frame per render. A co-op win holds both
    /// processes at one frame, which a pass of several frames would overshoot.
    pub(super) fn observes_every_frame(&self) -> bool {
        self.battle.is_some()
            || self
                .cooperative
                .as_ref()
                .is_some_and(cooperative::CooperativeRun::flags_win)
    }

    pub(super) fn owns_simulation_steps(&self, state: &AppState) -> bool {
        // Keep the final observed frame stable during GPU readback. Ordinary
        // App frame pacing and NetworkCommandBoundary own every earlier step,
        // except the co-op win frame's hold.
        self.ready()
            || self
                .cooperative
                .as_ref()
                .is_some_and(|run| run.holds_simulation(state))
    }

    pub(super) fn after_present(
        &mut self,
        state: &mut AppState,
        rendered: PresentedShell,
        frame: u32,
    ) -> Result<()> {
        if matches!(
            self.target,
            LanTarget::LiveVeraCoopHost | LanTarget::LiveVeraCoopGuest
        ) && matches!(
            self.phase,
            Phase::Discover | Phase::HostSetup | Phase::HostReady | Phase::GuestSetup
        ) {
            let started = *self.coop_setup_started.get_or_insert_with(Instant::now);
            if started.elapsed() >= VERA_COOP_SETUP_WATCHDOG {
                anyhow::bail!(
                    "live VERA Coop setup watchdog expired after {}s: {}",
                    VERA_COOP_SETUP_WATCHDOG.as_secs(),
                    vera_coop_setup_watchdog_snapshot(self, state, rendered)
                );
            }
        }
        if self.target == LanTarget::LiveVeraCoopHost
            && self.phase == Phase::Loading
            && state.frontend.screen != GameScreen::Loading
            && state.frontend.screen != GameScreen::InGame
            && frame.saturating_sub(self.coop_setup_started_frame.unwrap_or(frame))
                > VERA_COOP_START_WATCHDOG_FRAMES
        {
            anyhow::bail!(
                "VERA Coop Start did not leave setup within {VERA_COOP_START_WATCHDOG_FRAMES} render frames: {}",
                vera_coop_setup_watchdog_snapshot(self, state, rendered)
            );
        }
        if let Some(latency) = self.latency.as_mut() {
            latency.observe_setup(state);
        }
        ensure!(
            state.frontend.main_menu_shell_error.is_none(),
            "live LAN capture reached shell fallback"
        );
        // Only the explicitly armed external desync run accepts checksum CE.
        // The disappearance runs still require the ordinary connection EA.
        let expected_checksum = self
            .failure
            .as_ref()
            .is_some_and(|failure| failure.expects_checksum());
        let expected_startup = self
            .failure
            .as_ref()
            .is_some_and(|failure| failure.is_startup() && failure.armed());
        ensure!(
            state.match_state.network_failure.is_none() || expected_checksum || expected_startup,
            "unexpected network failure modal during live capture"
        );
        if let Some(runtime) = &state.match_state.sim_runtime {
            ensure!(
                runtime.simulation.pending_network_failure().is_none() || expected_checksum,
                "live LAN checksum failure: {:?}",
                runtime.simulation.pending_network_failure()
            );
            ensure!(
                runtime.simulation.pending_network_interruption().is_none()
                    || (expected_startup
                        && runtime.simulation.pending_network_interruption()
                            == Some(crate::sim::world::NetworkInterruption::ConnectionFailed)),
                "live LAN interrupted: {:?}",
                runtime.simulation.pending_network_interruption()
            );
        }
        if rendered == PresentedShell::NetworkSetup {
            self.observe_map_admission(state)?;
        }
        if let Some(mut rounds) = self.rounds.take() {
            let controls = rounds.controls(self);
            let result = if controls {
                rounds.after_present(self, state, rendered, frame)
            } else {
                Ok(())
            };
            self.rounds = Some(rounds);
            if controls {
                return result;
            }
        }
        if let Some(mut crash) = self.setup_crash.take() {
            // This pre-GO checkpoint holds ordinary Accept/Start/kick inputs,
            // while the same LAN poll/map/lease owners continue normally. In
            // particular, its wait cannot hit the generic1000-frame join guard.
            let result = crash.after_present(self, state, rendered, frame);
            self.setup_crash = Some(crash);
            if result? {
                return Ok(());
            }
        }
        if !self.guest_watched
            && state
                .match_state
                .session
                .network_setup()
                .is_some_and(|launch| {
                    launch
                        .humans
                        .iter()
                        .any(|human| human.name == GUEST_NAME && human.is_observer())
                })
        {
            self.guest_watched = true;
            self.event(frame, "the guest watches from a spectator row");
        }
        if frame % 120 == 0 {
            log::debug!(
                "live LAN {:?} render {frame}: phase={:?}, screen={:?}, setup={}",
                self.target,
                self.phase,
                state.frontend.screen,
                state
                    .match_state
                    .session
                    .network_setup()
                    .map_or(0, |launch| launch.humans.len())
            );
        }
        let last_action_frame = self
            .route
            .last()
            .and_then(|event| event["render_frame"].as_u64())
            .unwrap_or(0) as u32;
        if frame.saturating_sub(last_action_frame) >= 1000
            && matches!(self.phase, Phase::GuestSetup | Phase::HostReady)
            && state
                .match_state
                .session
                .network_setup()
                .is_none_or(|launch| launch.humans.len() < 2)
        {
            let lobby = state.frontend.lan.as_ref();
            anyhow::bail!(
                "live LAN admission stalled: phase={:?}, pending={:?}, modal={:?}, lobby={:?}",
                self.phase,
                lobby.and_then(|lobby| lobby.pending_game()),
                lobby.and_then(|lobby| lobby.modal),
                lobby.map(|lobby| &lobby.lobby)
            );
        }
        match self.phase {
            Phase::Main
                if rendered == PresentedShell::MainMenu
                    && steady_main_menu_capture_ready_for(
                        MainMenuCaptureSnapshot::from_state(state),
                        self.latency.is_some(),
                    )? =>
            {
                // Hidden-window transcript captures suppress physical LAN.
                // Live captures install the real owner before normal Network
                // input opens the shell; it retains sockets through gameplay.
                let isolated_ra2 = LanCapture::isolated_root(state)?;
                if matches!(
                    self.target,
                    LanTarget::LiveVeraCoopHost | LanTarget::LiveVeraCoopGuest
                ) {
                    let isolated_settings = LanCapture::isolated_settings_root(state)?;
                    ensure!(
                        isolated_ra2 != isolated_settings,
                        "VERA Coop setup capture needs separate isolated assets and settings roots"
                    );
                }
                ensure!(
                    state.match_state.lan_service.is_none(),
                    "LAN transport was already installed"
                );
                let config = self.config.as_ref().expect("LAN transport configuration");
                let mut service = match config.transport {
                    LiveLanTransport::ProductionBroadcast => {
                        crate::app::lan_service::LanService::open(
                            crate::app::match_runtime::frame_pacer::wall_clock_bucket(),
                        )?
                    }
                    LiveLanTransport::Explicit { bind, peer } => {
                        let alternate = std::net::SocketAddrV4::new(*bind.ip(), bind.port() + 2);
                        crate::app::lan_service::LanService::open_on(
                            UdpBindConfig {
                                primary: bind,
                                alternate,
                            },
                            &[peer],
                        )?
                    }
                };
                service.start_io_pump()?;
                state.match_state.lan_service = Some(service);
                click_main_menu(
                    state,
                    crate::ui::main_menu_shell::MainMenuControlId::Network0x578,
                )?;
                self.event(
                    frame,
                    "real UDP service installed; MainMenu Network clicked",
                );
                self.phase = Phase::Lobby;
            }
            Phase::Lobby if rendered == PresentedShell::LanLobby && lobby_settled(state) => {
                let name = if self.host() { HOST_NAME } else { GUEST_NAME };
                lobby_click(state, lobby::layout(state).name);
                lobby::key_press(state, Some(KeyCode::Home), None);
                for _ in 0..32 {
                    lobby::key_press(state, Some(KeyCode::Delete), None);
                }
                lobby::key_press(state, None, Some(name));
                if state.frontend.lan.as_ref().context("BB")?.lobby.observer {
                    lobby_click(state, lobby::layout(state).observer_toggle);
                }
                self.event(frame, "BB name entered through normal input");
                if self.host() {
                    create_lan_game(state)?;
                    self.phase = Phase::HostSetup;
                } else {
                    self.phase = Phase::Discover;
                }
            }
            Phase::Discover if rendered == PresentedShell::LanLobby && lobby_settled(state) => {
                let found = state
                    .frontend
                    .lan
                    .as_ref()
                    .context("BB")?
                    .lobby
                    .games
                    .iter()
                    .position(|game| game.name == HOST_NAME && game.open);
                if let Some(index) = found {
                    // Index0 is BB's synthetic no-game row. Drive its normal
                    // selection handler, which emits the real query packet.
                    let layout = lobby::layout(state);
                    let lobby_state = &state.frontend.lan.as_ref().context("BB")?.lobby;
                    let geometry = lobby_state.game_list.geometry(
                        layout.games,
                        lobby_state.game_count(),
                        layout.page.scale,
                    );
                    let (x, y) =
                        list_row_point(geometry, index + 1, lobby_state.game_list.top_index);
                    lobby_click(state, RectPx::new(x, y, 2, 2));
                    lobby_button(state, crate::ui::lan_shell::JOIN)?;
                    self.event(frame, "discovered host selected; real Join request");
                    self.phase = Phase::GuestSetup;
                }
            }
            Phase::HostSetup => {
                let selected = self.config.as_ref().and_then(|config| config.map.clone());
                let mode_id = if self.target == LanTarget::LiveVeraCoopHost {
                    Some(vera_cooperative_mode_id(state)?)
                } else {
                    None
                };
                let prepared = if let Some(mut run) = self.cooperative.take() {
                    let result = run.prepare_host(state, rendered, &mut self.route, frame);
                    self.cooperative = Some(run);
                    let used = result?;
                    if used {
                        self.open_human_rows(state, None)?;
                    }
                    used
                } else if let Some(mut rounds) = self.rounds.take() {
                    let result =
                        rounds.prepare_host_map(self, state, rendered, frame, selected.as_deref());
                    self.rounds = Some(rounds);
                    result?
                } else if rendered == PresentedShell::NetworkSetup && setup_settled(state, true) {
                    if self.target == LanTarget::LiveVeraCoopHost {
                        // The production chooser runs this same discovery owner
                        // when its VERA Coop mode row is selected; the
                        // diagnostic commits the discovered Entry map directly
                        // through the normal Use Map transaction.
                        crate::app::shell_network_setup::refresh_vera_coop_campaign_entries(state);
                    }
                    self.prepare_host_setup(
                        state,
                        selected.as_deref(),
                        self.expected_players,
                        mode_id,
                        None,
                    )?;
                    true
                } else {
                    false
                };
                if prepared {
                    self.event(
                        frame,
                        if self.target == LanTarget::LiveVeraCoopHost {
                            "host selected the VERA Coop map through production setup owner"
                        } else {
                            "host selected the map and open human rows through normal setup"
                        },
                    );
                    self.phase = Phase::HostReady;
                }
            }
            Phase::HostReady
                if rendered == PresentedShell::NetworkSetup && setup_settled(state, true) =>
            {
                if self.target == LanTarget::LiveVeraCoopHost {
                    self.step_vera_coop_host_setup(state, frame)?;
                    return Ok(());
                }
                let session = &state.match_state.session;
                if let Some(launch) = session.network_setup()
                    && launch.humans.len() == 2
                    && launch
                        .humans
                        .iter()
                        .all(|human| human.raw_slot_index.is_some())
                    && launch.humans.iter().enumerate().all(|(index, _)| {
                        session.network_player_ready(index)
                            && session.network_map_status(index) == Some(1)
                    })
                    && self.choices_made(state, false)
                    && (!self.waits_for_spectator()
                        || self.guest_watched
                            && launch.humans.iter().all(|human| !human.is_observer()))
                {
                    setup_button(state, crate::ui::multiplayer_setup::START)?;
                    self.event(frame, "all real acknowledgements present; Start clicked");
                    self.phase = Phase::Loading;
                }
            }
            Phase::GuestSetup
                if rendered == PresentedShell::NetworkSetup && setup_settled(state, false) =>
            {
                if self.target == LanTarget::LiveVeraCoopGuest {
                    self.step_vera_coop_guest_setup(state, frame)?;
                    return Ok(());
                }
                // The guest's game accepts the received options and map by
                // itself, and again after each host change, so the run stays
                // in editable setup until the host starts.
                self.select_country(state)?;
                self.switch_sides(state, frame)?;
            }
            Phase::Loading | Phase::GuestSetup if state.frontend.screen == GameScreen::InGame => {
                ensure!(
                    state.match_state.live_network.is_some(),
                    "loaded LAN match has no real command transport"
                );
                self.startup = Some(startup_receipt(state)?);
                if let Some(run) = self.cooperative.as_mut() {
                    run.match_loaded(state);
                }
                self.event(
                    frame,
                    "real GO/loading finished; ordinary simulation running",
                );
                self.phase = Phase::Running;
            }
            _ => {}
        }
        if self.phase == Phase::Running {
            if let Some(run) = self.cooperative.as_mut() {
                run.step(state, &mut self.route, frame)?;
                if run.done() {
                    self.phase = Phase::Finished;
                }
            } else {
                self.observe_run(state, frame, self.expected_players)?;
            }
        }
        Ok(())
    }

    fn observe_map_admission(&mut self, state: &AppState) -> Result<()> {
        let session = &state.match_state.session;
        let Some(launch) = session.network_setup() else {
            return Ok(());
        };
        let (Some(file), Some(metadata), Some(source), Some(effective)) = (
            launch.selected_map_file.as_deref(),
            session.network_map_metadata(),
            session.network_local_map_source(),
            session.network_local_map_effective_digest(),
        ) else {
            return Ok(());
        };
        if session.network_map_status(launch.local_player_index) != Some(1)
            || source.physical_identity()
                != Some((
                    usize::try_from(metadata.byte_length)?,
                    metadata.source_sha256.as_str(),
                ))
            || effective != metadata.effective_digest
        {
            return Ok(());
        }
        let protocol = state
            .frontend
            .network_setup_protocol
            .as_ref()
            .context("verified setup protocol")?;
        let generated = crate::map::rmg::is_seed_selection(file);
        let staging = if generated {
            if session.network_random_map_preview().is_none() {
                return Ok(());
            }
            let text = state
                .frontend
                .multiplayer_shell_runtime
                .start_staging()
                .to_w_text();
            let checked = crate::app::shell_network_setup::maps::checked_staging(
                &text,
                metadata.player_capacity,
            )?;
            Some(
                json!({"text":text,"digest":crate::app::shell_network_setup::maps::staging_digest(state),
                "count":checked.count,"width":checked.width,"height":checked.height}),
            )
        } else {
            None
        };
        self.map_admission = Some(json!({"file":file,"generated":generated,"source":source,
        "source_sha256":metadata.source_sha256,"byte_length":metadata.byte_length,
        "effective_digest":effective,"capacity":metadata.player_capacity,
        "preview_source":session.network_random_map_preview(),"staging":staging,
        "preview_surface":protocol.received_preview().map(|(_,image)|json!({"width":image.width,"height":image.height})),
        "preview_bounds":protocol.received_preview_bounds().map(|bounds|json!({
            "origin":[bounds.origin_x,bounds.origin_y],"width":bounds.width,"height":bounds.height,
            "starts":bounds.start_points.iter().map(|point|[point.x,point.y]).collect::<Vec<_>>()
        }))}));
        Ok(())
    }
    fn prepare_host_setup(
        &mut self,
        state: &mut AppState,
        selected_map: Option<&str>,
        players: usize,
        mode_id: Option<i32>,
        ai: Option<crate::skirmish_launch::AiDifficulty>,
    ) -> Result<()> {
        let mode = state
            .frontend
            .skirmish_modes
            .iter()
            .find(|mode| {
                mode_id.map_or(
                    mode.class == crate::skirmish_modes::MpModeClass::Battle,
                    |id| mode.id == id,
                )
            })
            .context("requested retail network mode")?;
        let mode_id = mode.id;
        let map_index = state
            .frontend
            .scenario_catalog
            .records()
            .iter()
            .position(|map| {
                (map.kind == crate::map::skirmish_scenarios::SkirmishScenarioKind::ConcreteMap
                    || selected_map.is_some_and(|file| file.eq_ignore_ascii_case("RandMap.Sed")))
                    && map.player_capacity >= (players + usize::from(ai.is_some())) as i32
                    && selected_map.is_none_or(|file| map.file_name.eq_ignore_ascii_case(file))
                    && crate::map::skirmish_scenarios::is_valid_map(map, mode, false)
            })
            .context("retail network map with enough starts")?;
        setup::commit_map_selection(state, mode_id, map_index, None)?;
        self.select_country(state)?;
        self.open_human_rows(state, ai)
    }

    fn step_vera_coop_host_setup(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        let session = &state.match_state.session;
        let launch = session.network_setup().context("VERA Coop host setup")?;
        if launch.humans.len() < 2 {
            return Ok(());
        }
        ensure!(
            session.host()
                && launch.mode.class == crate::skirmish_modes::MpModeClass::VeraCooperative,
            "live Coop host lost its admitted mode"
        );
        let (host, guest) = coop_setup_rows(launch)?;
        let host_ready = session.network_player_ready(host);
        let guest_ready = session.network_player_ready(guest);
        let host_map = session.network_map_status(host) == Some(1);
        let guest_map = session.network_map_status(guest) == Some(1);

        if self.coop_setup_initial.is_none() {
            if !host_ready || !host_map {
                return Ok(());
            }
            self.coop_setup_initial = Some(vera_coop_setup_snapshot(state)?);
            self.event(
                frame,
                if guest_ready && guest_map {
                    "host snapshot captured after guest Ready/map status had already arrived"
                } else {
                    "host map/config accepted; waiting for guest Ready and map status"
                },
            );
            if !guest_ready || !guest_map {
                return Ok(());
            }
        }
        if !host_ready || !guest_ready || !host_map || !guest_map {
            return Ok(());
        }
        let final_snapshot = vera_coop_setup_snapshot(state)?;
        ensure_coop_setup_ready_transition(
            self.coop_setup_initial
                .as_ref()
                .context("initial VERA Coop host snapshot")?,
            &final_snapshot,
            true,
        )?;
        self.coop_setup_final = Some(final_snapshot.clone());
        self.route.push(json!({
            "render_frame": frame,
            "action": "guest Ready received by host over real LAN",
            "state": final_snapshot,
        }));
        // mode+14's VERA Coop admission validates the frozen role/House roster
        // against the effective map, then freezes it for the loader's own
        // re-verification and the shared command grants.
        setup_button(state, crate::ui::multiplayer_setup::START)?;
        self.event(
            frame,
            "host Start clicked; VERA Coop admission and frozen start",
        );
        self.coop_setup_started_frame = Some(frame);
        self.phase = Phase::Loading;
        Ok(())
    }

    fn step_vera_coop_guest_setup(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        let session = &state.match_state.session;
        let launch = session.network_setup().context("VERA Coop guest setup")?;
        if launch.mode.class != crate::skirmish_modes::MpModeClass::VeraCooperative
            || launch.selected_map_file.is_none()
        {
            return Ok(());
        }
        ensure!(!session.host(), "live Coop guest lost the admitted mode");
        if self.coop_setup_final.is_some() {
            return Ok(());
        }
        let (_, guest) = coop_setup_rows(launch)?;
        if !self.coop_setup_guest_accepted {
            if session.network_map_status(guest) != Some(1)
                || !setup::view(state)
                    .context("VERA Coop guest view")?
                    .accept_enabled
            {
                return Ok(());
            }
            ensure!(
                !session.network_player_ready(guest),
                "guest was already Ready before the diagnostic Accept"
            );
            self.coop_setup_initial = Some(vera_coop_setup_snapshot(state)?);
            setup_button(state, crate::ui::multiplayer_setup::ACCEPT)?;
            let accepted = state
                .match_state
                .session
                .network_setup()
                .context("accepted Coop guest setup")?;
            let (_, guest) = coop_setup_rows(accepted)?;
            ensure!(
                state.match_state.session.network_player_ready(guest),
                "production Accept did not set guest Ready"
            );
            self.coop_setup_guest_accepted = true;
            // Record the guest-local Ready transition immediately: the host
            // may publish GO before this step runs again, which would skip the
            // later final snapshot and leave the receipt incomplete.
            let initial = self
                .coop_setup_initial
                .clone()
                .context("initial VERA Coop guest snapshot")?;
            let final_snapshot = vera_coop_setup_snapshot(state)?;
            ensure_coop_setup_ready_transition(&initial, &final_snapshot, false)?;
            self.coop_setup_final = Some(final_snapshot.clone());
            self.route.push(json!({
                "render_frame": frame,
                "action": "guest local Ready confirmed after real host options and map acceptance",
                "state": final_snapshot,
            }));
            self.event(
                frame,
                "guest accepted the host's VERA Coop map through production setup",
            );
            return Ok(());
        }

        let initial = self
            .coop_setup_initial
            .as_ref()
            .context("initial VERA Coop guest snapshot")?;
        if session.network_map_status(guest) != Some(1) || !session.network_player_ready(guest) {
            return Ok(());
        }
        let final_snapshot = vera_coop_setup_snapshot(state)?;
        ensure_coop_setup_ready_transition(initial, &final_snapshot, false)?;
        self.coop_setup_final = Some(final_snapshot.clone());
        self.route.push(json!({
            "render_frame": frame,
            "action": "guest local Ready confirmed after real host options and map acceptance",
            "state": final_snapshot,
        }));
        // The guest stays in setup until the host's accepted Start publishes
        // GO; the shared loading/InGame transition below takes over then.
        Ok(())
    }

    /// Open every free row to a human through the setup's Player combos,
    /// keeping one `ai` row when one is requested.
    fn open_human_rows(
        &mut self,
        state: &mut AppState,
        ai: Option<crate::skirmish_launch::AiDifficulty>,
    ) -> Result<()> {
        // Use the existing action admission, including current control
        // availability. The isolated profile must allow one open peer.
        let mut retained_ai = false;
        for row in 1..crate::skirmish_launch::NETWORK_PLAYER_SLOT_COUNT {
            if state
                .match_state
                .session
                .network_setup()
                .is_some_and(|launch| {
                    launch
                        .humans
                        .iter()
                        .any(|human| human.raw_slot_index == Some(row as u8))
                })
            {
                continue;
            }
            if !retained_ai
                && ai.is_some()
                && state.match_state.session.network_display_ai(row as u8) == ai
            {
                retained_ai = true;
                continue;
            }
            let Some(control) = setup_control_at_row(state, SetupColumn::Player, row)? else {
                continue;
            };
            if setup::view(state)
                .and_then(|view| view.combo(control).cloned())
                .is_some_and(|combo| {
                    combo.visible
                        && combo.enabled
                        && combo.choices.iter().any(|choice| choice.value == 2)
                })
            {
                setup::apply_action(state, SetupAction::Select { control, value: 2 });
            }
        }
        // Keep ordinary host input on its local page after visiting the roster.
        let _ = setup_control_at_row(state, SetupColumn::Player, 0)?;
        let launch = state
            .match_state
            .session
            .network_setup()
            .context("host setup")?;
        ensure!(
            launch.opponents.is_empty()
                || ai.is_some_and(|difficulty| launch.opponents.len() == 1
                    && launch.opponents[0].difficulty == difficulty),
            "live diagnostic retained an unrequested AI row"
        );
        Ok(())
    }

    fn observe_run(
        &mut self,
        state: &mut AppState,
        frame: u32,
        active_players: usize,
    ) -> Result<()> {
        if let Some(failure) = self.failure.as_mut().filter(|failure| failure.armed()) {
            if failure.step(state, frame, &mut self.route)? {
                MoviesCapture::restore_neutral_pointer(state);
                self.phase = Phase::Finished;
            }
            return Ok(());
        }
        // After the injection the mission owner retires the world; the result
        // tail runs on render frames so it does not depend on another sim tick.
        if self.vera_outcome.is_some()
            && matches!(
                self.vera_outcome_phase,
                VeraOutcomePhase::Observe | VeraOutcomePhase::Quit
            )
        {
            #[cfg(feature = "dev-overlays")]
            {
                self.finish_vera_outcome(state)?;
                return Ok(());
            }
            #[cfg(not(feature = "dev-overlays"))]
            anyhow::bail!("--vera-outcome requires a dev-overlays build");
        }
        ensure!(
            state.frontend.screen == GameScreen::InGame || self.vera_outcome.is_some(),
            "live LAN match left gameplay before completion"
        );
        if self.battle.is_none() && self.mass.is_none() && self.latency.is_none() {
            let simulation = &state
                .match_state
                .sim_runtime
                .as_ref()
                .context("live simulation")?
                .simulation;
            // A pending native frame may already have applied commands, but
            // its post-logic position is not committed for observation yet.
            if simulation.pending_network_wait() {
                return Ok(());
            }
            self.observe_simple_input_applications(state, simulation.session().binary_frame())?;
        }
        let simulation = &state
            .match_state
            .sim_runtime
            .as_ref()
            .context("live simulation")?
            .simulation;
        let committed = simulation.session().binary_frame();
        if matches!(
            self.target,
            LanTarget::LiveVeraCoopHost | LanTarget::LiveVeraCoopGuest
        ) && self.actor.is_some()
            && self.pair_linger_at.is_none()
            && self.vera_outcome.is_none()
            && committed >= self.required_frames() + 600
        {
            anyhow::bail!(
                "VERA Coop capture incomplete at native frame {committed}: actions={}, stopped={}, stop_settled={}, checksums={}, simple_applied={}, move_applied={}",
                json!(self.actions),
                self.stopped,
                self.stop_observation.position().is_some(),
                self.checksums.len(),
                self.simple_inputs_applied(),
                self.move_input_applied()
            );
        }
        if let Some(ai) = self.ai.as_mut() {
            ai.observe(state)?;
        }
        if self.battle.is_none()
            && self.pair_linger_at.is_none()
            && let Some(actor) = self.actor
        {
            let entity = simulation
                .entities()
                .get(actor)
                .with_context(|| {
                    format!(
                        "commanded actor {actor} disappeared at native frame {committed}; last_seen={}; ai_activity={}",
                        json!(self.actor_last_seen),
                        json!(self.ai.as_ref().and_then(|ai| ai.activity.as_ref()))
                    )
                })?;
            self.actor_last_seen = Some(json!({"native_frame":committed,
                "type":simulation.interner().resolve(entity.type_ref()),
                "owner":simulation.interner().resolve(entity.owner()),
                "health":entity.health.current(),"alive":entity.is_alive(),
                "position":entity.position,"mission":entity.mission.current().raw(),
                "destination":entity.navigation.nav_com()}));
            self.observed_movement |= self
                .origin
                .is_some_and(|origin| origin != (entity.position.rx(), entity.position.ry()));
            self.stop_observation.observe(
                committed,
                self.actions.iter().any(|action| {
                    action["input"] == "Stop" && action.get("application_frame").is_some()
                }),
                serde_json::to_value(entity.position)?,
                entity.navigation.nav_com().is_none() && entity.navigation.nav_queue().is_empty(),
            );
        }
        ensure!(
            committed >= self.checksums.len() as u32,
            "native frame moved backwards"
        );
        ensure!(
            committed.min(self.required_frames()) - self.checksums.len() as u32 <= 256,
            "capture missed overwritten checksum history"
        );
        let required_frames = self.required_frames();
        while (self.checksums.len() as u32) < committed.min(required_frames) {
            self.checksums.push(
                state
                    .match_state
                    .session
                    .checksum_history()
                    .get(self.checksums.len() as u32),
            );
        }
        if committed >= required_frames && self.simulation_hashes.is_empty() {
            self.simulation_hashes =
                recorded_simulation_hashes(state, Some(u64::from(required_frames)))?;
        }
        let live = state.match_state.live_network.as_ref();
        // A LAN battle ends at its own committed observation prefix. The peer
        // can write that same prefix and exit before our next render; waiting
        // for its final advertisement races that ordinary transport teardown.
        // The pair runner still requires and compares both complete prefixes.
        let lan_battle_prefix =
            self.battle.is_some() && !self.target.online() && committed >= required_frames;
        // A VERA Coop peer holds its transport after its own completion so the
        // slower peer can settle; the peer's ordinary exit is expected here.
        let vera_pair_lingering = self.pair_linger_at.is_some()
            && matches!(
                self.target,
                LanTarget::LiveVeraCoopHost | LanTarget::LiveVeraCoopGuest
            );
        // A VERA outcome run keeps observing after the result dialog has taken
        // the LIVE transport into the co-op round owner, so the ordinary
        // transport checks apply only while it is still present.
        if let Some(live) = live {
            ensure!(
                lan_battle_prefix
                    || vera_pair_lingering
                    || self.vera_outcome.is_some()
                    || live.peers().len() + 1 == active_players,
                "live diagnostic peer disappeared"
            );
            let wait = format!("{:?}", live.wait_info());
            if self.last_wait.as_deref() != Some(&wait) {
                self.route
                    .push(json!({"render_frame":frame,"native_frame":committed,"wait":wait}));
                self.last_wait = Some(wait);
            }
        } else {
            ensure!(self.vera_outcome.is_some(), "real live transport");
        }
        if let Some(latency) = self.latency.as_mut() {
            if latency.step(state, required_frames)? {
                MoviesCapture::restore_neutral_pointer(state);
                self.phase = Phase::Finished;
            }
            return Ok(());
        }
        if let Some(mass) = self.mass.as_mut() {
            let complete = mass.step(state, committed)?;
            if committed >= required_frames {
                ensure!(
                    complete,
                    "mass input did not finish within the configured observation frames"
                );
                let live = state
                    .match_state
                    .live_network
                    .as_ref()
                    .context("live transport")?;
                if live.uses_sequenced_lockstep()
                    || live
                        .peers()
                        .iter()
                        .all(|peer| peer.frame() >= required_frames as i32)
                {
                    state
                        .match_state
                        .match_diagnostics
                        .end_command_observation();
                    MoviesCapture::restore_neutral_pointer(state);
                    self.phase = Phase::Finished;
                }
            }
            return Ok(());
        }
        if self.battle.is_some() {
            // Wait_For_Players is inside an unfinished native MainTick: Logic
            // may already have repaired a building, but DoList and the frame
            // commit are still pending. Observe only its committed result.
            if simulation.pending_network_wait() {
                return Ok(());
            }
            let host = self.host();
            let battle = self.battle.as_mut().expect("battle target");
            battle.step(state, committed, host, &mut self.actions)?;
            if committed >= required_frames {
                let missing = battle.missing(state)?;
                ensure!(
                    missing.is_empty(),
                    "Battle opening incomplete: {}; missing {}",
                    battle.phase(),
                    missing.join(", ")
                );
                let live = state
                    .match_state
                    .live_network
                    .as_ref()
                    .context("live transport")?;
                if lan_battle_prefix
                    || live.uses_sequenced_lockstep()
                    || live
                        .peers()
                        .iter()
                        .all(|peer| peer.frame() >= required_frames as i32)
                {
                    MoviesCapture::restore_neutral_pointer(state);
                    self.phase = Phase::Finished;
                }
            }
            return Ok(());
        }
        if matches!(
            self.target,
            LanTarget::LiveVeraCoopHost | LanTarget::LiveVeraCoopGuest
        ) && self.vera_probe.is_none()
            && committed >= 5
        {
            self.vera_probe = Some(vera_coop_probe(state)?);
        }
        if committed >= 10 && self.actor.is_none() {
            self.issue_move(state, committed)?;
        }
        if committed >= 80 && !self.stopped && self.move_input_applied() {
            let actor = self.actor.context("local move actor")?;
            battle::mobile_stop_with_keyboard(state, actor)?;
            self.stopped = true;
            self.actions
                .push(json!({"native_frame":committed,"input":"Stop","actor":actor}));
        }
        // The outcome run reuses the prefix inputs above and then injects its
        // mission result; it never takes the ordinary completion branch.
        if self.vera_outcome.is_some() {
            #[cfg(feature = "dev-overlays")]
            {
                self.step_vera_outcome(state, committed)?;
                return Ok(());
            }
            #[cfg(not(feature = "dev-overlays"))]
            anyhow::bail!("--vera-outcome requires a dev-overlays build");
        }
        let live = state
            .match_state
            .live_network
            .as_ref()
            .context("live transport")?;
        if self.checksums.len() >= self.required_frames() as usize
            && self.stopped
            && self.simple_inputs_applied()
            && self.stop_observation.position().is_some()
            // The survivor must arm from its own committed prefix: the
            // finishing guest may exit at300 before sending frame300's
            // advertisement. The pair receipt compares both300-frame rings.
            && (self.failure.is_some()
                // Sequenced delivery has independent reader cursors, rather
                // than native peer frame advertisements. The pair runner
                // compares both clients' actual committed prefix receipts.
                || live.uses_sequenced_lockstep()
                || live
                    .peers()
                    .iter()
                    .all(|peer| peer.frame() >= self.required_frames() as i32))
        {
            if let Some(ai) = &self.ai {
                ensure!(
                    ai.activity.is_some(),
                    "AI House had no observed factory progress or new live entity; snapshot={}",
                    ai.snapshot(state)?
                );
            }
            ensure!(
                self.observed_movement,
                "Move never changed the actor's cell"
            );
            let entity = state
                .match_state
                .sim_runtime
                .as_ref()
                .context("simulation")?
                .simulation
                .entities()
                .get(self.actor.context("actor")?)
                .context("actor")?;
            ensure!(
                self.stop_observation.position() == Some(&serde_json::to_value(entity.position)?)
                    && entity.navigation.nav_com().is_none()
                    && entity.navigation.nav_queue().is_empty(),
                "Stop did not retain a stable position through the final frames"
            );
            let peer_frames: Vec<i32> = live.peers().iter().map(|peer| peer.frame()).collect();
            MoviesCapture::restore_neutral_pointer(state);
            if let Some(failure) = self.failure.as_mut() {
                failure.arm(state)?;
                self.event(
                    frame,
                    if self.target.desync_failure() {
                        "shared 300-frame prefix complete; awaiting external scoped FRAMEINFO corruption"
                    } else {
                        "shared 300-frame prefix complete; host awaits real peer disappearance"
                    },
                );
            } else if matches!(
                self.target,
                LanTarget::LiveVeraCoopHost | LanTarget::LiveVeraCoopGuest
            ) && !self.pair_linger_done(committed, &peer_frames)
            {
                // Hold this process on the frame loop until the peer is past
                // its own completion margin; exiting first makes the slower
                // peer fail with "live diagnostic peer disappeared".
            } else {
                self.phase = Phase::Finished;
            }
        }
        Ok(())
    }

    /// Whether this VERA Coop process may leave the frame loop: the peer has
    /// advertised the linger margin after our own completion, or the wall
    /// clock wait expired. The first observation is recorded once.
    fn pair_linger_done(&mut self, committed: u32, peer_frames: &[i32]) -> bool {
        let Some(started) = self.pair_linger_at else {
            self.pair_linger_at = Some(committed);
            self.pair_linger_wall = Some(Instant::now());
            self.event(
                committed,
                "VERA Coop pair conditions met; holding the transport until the peer settles",
            );
            return false;
        };
        let _ = started;
        let peer_gone = peer_frames.is_empty();
        let waited = self
            .pair_linger_wall
            .is_some_and(|wall| wall.elapsed() >= VERA_COOP_PAIR_LINGER_WAIT);
        if !peer_gone && !waited {
            return false;
        }
        self.route.push(json!({
            "native_frame": committed,
            "input": "VERA Coop pair complete",
            "linger_frames": committed.saturating_sub(self.pair_linger_at.unwrap_or(committed)),
            "peer_gone": peer_gone,
        }));
        true
    }

    /// Whether this capture injects a VERA mission outcome.
    pub(super) fn vera_outcome_enabled(&self) -> bool {
        self.vera_outcome.is_some()
    }

    /// Checkpoint screenshot labels requested since the last drain.
    pub(super) fn take_screenshot_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.screenshot_requests)
    }

    fn request_screenshot(&mut self, label: impl Into<String>) {
        self.screenshot_requests.push(label.into());
    }

    /// The VERA outcome diagnostic: keep the mission prefix evidence, inject
    /// one outcome through the same production owner a map action reaches,
    /// then observe the result dialog (or the continued mission). Both peers
    /// inject at the same absolute committed frame, so the shared world sees
    /// one deterministic mutation.
    #[cfg(feature = "dev-overlays")]
    fn step_vera_outcome(&mut self, state: &mut AppState, committed: u32) -> Result<()> {
        use crate::sim::house_state::HouseOutcomeKind;

        let outcome = self.vera_outcome.context("VERA outcome diagnostic")?;
        let kind = outcome.as_str();
        match self.vera_outcome_phase {
            VeraOutcomePhase::Prefix => {
                if committed >= VERA_OUTCOME_MISSION_SHOT_FRAME {
                    ensure!(
                        self.checksums.len() >= self.required_frames() as usize
                            && self.stopped
                            && self.simple_inputs_applied()
                            && self.observed_movement
                            && self.stop_observation.position().is_some(),
                        "VERA outcome prefix was incomplete before its screenshot: frame={committed}, checksums={}/{}, stopped={}, inputs={}, moved={}, stop_settled={}",
                        self.checksums.len(),
                        self.required_frames(),
                        self.stopped,
                        self.simple_inputs_applied(),
                        self.observed_movement,
                        self.stop_observation.position().is_some()
                    );
                    self.request_screenshot("vera-mission");
                    // Build the receipt while the world and its roster are still
                    // resident; the outcome run retires the match before exiting.
                    self.final_receipt = Some(self.vera_receipt(state)?);
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
                        .as_mut()
                        .context("live simulation")?;
                    // Direct field projections keep the rules borrow disjoint
                    // from the mutable simulation borrow.
                    let rules = &runtime.resources.rules;
                    match outcome {
                        VeraCoopOutcome::Win => {
                            runtime.simulation.dev_trigger_current_house_outcome(
                                &rules,
                                HouseOutcomeKind::Victory,
                            );
                        }
                        VeraCoopOutcome::Lose => {
                            runtime.simulation.dev_trigger_current_house_outcome(
                                &rules,
                                HouseOutcomeKind::Defeat,
                            );
                        }
                        VeraCoopOutcome::HouseLoss => {
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
                    }
                    self.vera_outcome_injected_frame = Some(committed);
                    self.vera_outcome_phase = VeraOutcomePhase::Observe;
                    self.vera_outcome_render_frames = 0;
                    self.event(
                        committed,
                        match outcome {
                            VeraCoopOutcome::Win => "VERA outcome: team victory injected",
                            VeraCoopOutcome::Lose => "VERA outcome: team defeat injected",
                            VeraCoopOutcome::HouseLoss => {
                                "VERA outcome: every player House marked defeated"
                            }
                        },
                    );
                }
            }
            VeraOutcomePhase::Observe | VeraOutcomePhase::Quit => {}
        }
        Ok(())
    }

    /// After the injection the app retires the mission on its own; this tail
    /// only observes the result dialog (or the continued mission) in render
    /// frames, so it keeps running while the shared simulation is paused.
    #[cfg(feature = "dev-overlays")]
    fn finish_vera_outcome(&mut self, state: &mut AppState) -> Result<()> {
        self.vera_outcome_render_frames = self.vera_outcome_render_frames.saturating_add(1);
        let outcome = self.vera_outcome.context("VERA outcome diagnostic")?;
        let kind = outcome.as_str();
        let render_frame = self.vera_outcome_render_frames;
        match self.vera_outcome_phase {
            VeraOutcomePhase::Observe => match outcome {
                VeraCoopOutcome::Win | VeraCoopOutcome::Lose => {
                    if let Some(page) = state.frontend.coop_score_page.as_ref() {
                        let victory = page.victory;
                        ensure!(
                            victory == (outcome == VeraCoopOutcome::Win),
                            "VERA Coop result dialog reported the wrong verdict"
                        );
                        ensure!(
                            !page.model.can_continue,
                            "VERA Coop's first result dialog must not offer Continue"
                        );
                        self.vera_outcome_evidence = Some(json!({
                            "kind": kind,
                            "injected_frame": self.vera_outcome_injected_frame,
                            "dialog": {
                                "victory": victory,
                                "rows": page.model.rows.len(),
                                "can_continue": page.model.can_continue,
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
                            "VERA outcome: cooperative result dialog opened",
                        );
                        self.vera_outcome_dialog_frame = Some(render_frame);
                        self.vera_outcome_phase = VeraOutcomePhase::Quit;
                    } else {
                        ensure!(
                            render_frame < 900,
                            "VERA Coop result dialog did not open after the injected outcome"
                        );
                    }
                }
                VeraCoopOutcome::HouseLoss => {
                    ensure!(
                        state.frontend.screen == GameScreen::InGame
                            && state.frontend.coop_score_page.is_none()
                            && state.match_state.scenario_exit.is_none(),
                        "VERA Coop ended after every player House was marked defeated"
                    );
                    if render_frame >= VERA_OUTCOME_CONTINUED_FRAMES {
                        let continued_native_frame = state
                            .match_state
                            .sim_runtime
                            .as_ref()
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
            VeraOutcomePhase::Quit => {
                let opened = self
                    .vera_outcome_dialog_frame
                    .context("VERA result dialog frame")?;
                if render_frame >= opened.saturating_add(4) {
                    crate::app::cooperative_round::apply_action(
                        state,
                        crate::ui::coop_score_shell::CoopScoreAction::Quit,
                    );
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
                        "VERA outcome: Quit sent from the cooperative result dialog",
                    );
                    self.phase = Phase::Finished;
                }
            }
            VeraOutcomePhase::Prefix | VeraOutcomePhase::Inject => {}
        }
        Ok(())
    }

    fn issue_move(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        // Authored campaign maps may open with scripted input-lock actions
        // (trigger actions 46/47). Wait for the map's own unlock instead of
        // failing the diagnostic; the observation retries every frame until an
        // actor is retained.
        if crate::app::input::campaign_input::is_locked(state) {
            if self.locked_frames.is_none() {
                self.locked_frames = Some(frame);
                self.event(
                    frame,
                    "authored mission locked tactical input; waiting for its unlock",
                );
            }
            return Ok(());
        }
        if let Some(locked) = self.locked_frames.take() {
            self.route.push(json!({
                "native_frame": frame,
                "input": "tactical Move deferred",
                "input_locked_frames": frame.saturating_sub(locked),
            }));
        }
        state
            .match_state
            .match_diagnostics
            .begin_command_observation();
        let (actor, from, to) = battle::mobile_move_with_mouse(state)?;
        self.actor = Some(actor);
        self.origin = Some(from);
        self.actions.push(json!({"native_frame":frame,"input":"tactical Move","actor":actor,"from":from,"target":to}));
        Ok(())
    }

    fn simple_inputs_applied(&self) -> bool {
        self.actions.len() == 2
            && self
                .actions
                .iter()
                .all(|action| action.get("application_frame").is_some())
    }

    /// Whether the Move command reached a receiver application observation.
    fn move_input_applied(&self) -> bool {
        self.actions.iter().any(|action| {
            action.get("input").and_then(|input| input.as_str()) == Some("tactical Move")
                && action.get("application_frame").is_some()
        })
    }

    fn observe_simple_input_applications(
        &mut self,
        state: &mut AppState,
        frame: u32,
    ) -> Result<()> {
        for execution in state
            .match_state
            .match_diagnostics
            .take_command_observations()?
        {
            self.observe_simple_input_application(frame, execution)?;
        }
        if self.ai.is_some() {
            for action in &self.actions {
                let issued = action["native_frame"]
                    .as_u64()
                    .context("input issue frame")?;
                ensure!(
                    action.get("application_frame").is_some()
                        || u64::from(frame) <= issued + u64::from(SOLO_AI_INPUT_BOUND_FRAMES),
                    "solo AI {} has no receiver application within {SOLO_AI_INPUT_BOUND_FRAMES} native frame of click at {issued}; observed frame {frame}",
                    action["input"]
                );
            }
        }
        if self.stopped && self.simple_inputs_applied() {
            state
                .match_state
                .match_diagnostics
                .end_command_observation();
        }
        Ok(())
    }

    fn observe_simple_input_application(
        &mut self,
        frame: u32,
        execution: crate::app::match_diagnostics::CommandExecutionObservation,
    ) -> Result<()> {
        use crate::sim::command::{MegaMissionOrder, MegaMissionRecord, StopRecord};
        use crate::sim::components::NavTargetRef;
        use crate::sim::mission::{MissionId, MissionType};
        let Some(actor) = self.actor else {
            return Ok(());
        };
        if execution.actor != Some(actor) {
            return Ok(());
        }
        let (input, target) = if let Some(command) = MegaMissionRecord::decode(&execution.record) {
            let MegaMissionOrder::Move { target_x, target_y } = command.order else {
                return Ok(());
            };
            (
                "tactical Move",
                Some((u16::try_from(target_x)?, u16::try_from(target_y)?)),
            )
        } else if StopRecord::decode(&execution.record).is_some() {
            ("Stop", None)
        } else {
            return Ok(());
        };
        let Some(action) = self
            .actions
            .iter_mut()
            .find(|action| action["input"] == input)
        else {
            return Ok(());
        };
        let issued = u32::try_from(
            action["native_frame"]
                .as_u64()
                .context("input issue frame")?,
        )?;
        ensure!(
            execution.applied,
            "simple {input} receiver rejected actor {actor}"
        );
        ensure!(
            action.get("application_frame").is_none(),
            "duplicate simple {input} application"
        );
        // The production callback records the dispatch frame, before commit.
        // Online input is scheduled against the relay clock, so neither the
        // local click frame nor MaxAhead bounds when a catching-up client sees it.
        let delay = execution
            .frame
            .checked_sub(issued)
            .context("input applied before its click")?;
        if self.ai.is_some() {
            ensure!(
                delay <= SOLO_AI_INPUT_BOUND_FRAMES,
                "solo AI {input} applied {delay} native frames after click at {issued}; scheduled={}, applied={}",
                execution.record.frame_stamp(),
                execution.frame
            );
        }
        if let Some((x, y)) = target {
            ensure!(
                action["target"] == json!([x, y]),
                "Move receiver target differs from tactical click"
            );
            let moving = MissionId::from_known(MissionType::Move).raw();
            ensure!(
                (execution.destination == Some(NavTargetRef::cell(x, y))
                    && execution
                        .mission
                        .is_some_and(|(current, queued)| current == moving || queued == moving))
                    || (execution.cell == Some((x, y)) && execution.destination.is_none()),
                "Move receiver did not install or reach the clicked destination"
            );
        } else {
            ensure!(
                execution.mission.is_some() && execution.destination.is_none(),
                "Stop receiver retained a destination"
            );
        }
        action["application_frame"] = json!(execution.frame);
        action["application_observed_at_frame"] = json!(frame);
        action["scheduled_frame"] = json!(execution.record.frame_stamp());
        action["delay_frames"] = json!(delay);
        action["applied"] = json!(execution.applied);
        action["receiver"] = json!({"destination":execution.destination,"cell":execution.cell,"mission":execution.mission});
        action["timing"] = json!(execution.timing.as_ref().map(|timing| json!({
            "fps":timing.fps,"frame_send_rate":timing.frame_send_rate,"max_ahead":timing.max_ahead})));
        log::info!(
            "Simple capture {input}: issued={issued}, scheduled={}, applied={}, observed={frame}",
            execution.record.frame_stamp(),
            execution.frame
        );
        Ok(())
    }

    /// An incomplete opt-in latency round. Its partial receipt is still
    /// written before the capture outcome fails.
    pub(super) fn measurement_failure(&self) -> Option<String> {
        self.rounds
            .as_ref()?
            .latency_failure()
            .map(|failure| format!("online latency measurement incomplete in {failure}"))
    }

    fn vera_receipt(&self, state: &AppState) -> Result<Value> {
        ensure!(
            self.coop_setup_initial.is_some()
                && self.coop_setup_final.is_some()
                && self.startup.is_some(),
            "live VERA Coop capture did not reach the running mission: coop_initial={}, coop_final={}, startup={}",
            self.coop_setup_initial.is_some(),
            self.coop_setup_final.is_some(),
            self.startup.is_some()
        );
        let mut receipt = self.observed_receipt(state)?;
        receipt["scope"] = json!(
            "VERA Coop LAN: production host map/config commit and LAN options, both real Ready rows, mode+14 start admission, frozen role/House grants, then 300 native frames with local Move/Stop and a peer-House control probe; bounded runtime validation, not whole-match/native parity"
        );
        receipt["coop_setup"] = json!({
            "initial": self.coop_setup_initial,
            "after_guest_ready": self.coop_setup_final,
            "start_clicked": self.coop_setup_started_frame.is_some(),
            "start_clicked_frame": self.coop_setup_started_frame,
        });
        receipt["vera_coop"] = vera_coop_runtime_receipt(state, self.vera_probe.as_ref())?;
        Ok(receipt)
    }

    pub(super) fn receipt(&self, state: &AppState) -> Result<Value> {
        if let Some(crash) = &self.setup_crash {
            return crash.receipt();
        }
        if matches!(
            self.target,
            LanTarget::LiveVeraCoopHost | LanTarget::LiveVeraCoopGuest
        ) {
            // An outcome run retires the match before the process finishes, so
            // its receipt was built while the world and roster were resident.
            if let Some(receipt) = &self.final_receipt {
                return Ok(receipt.clone());
            }
            return self.vera_receipt(state);
        }
        if let Some(run) = &self.cooperative {
            if let Some(online) = &self.rounds {
                return Ok(json!({"target":format!("{:?}",self.target),
                    "service_boundary":"two independent App processes; normal WOL UI, deployed HTTPS lobby, authenticated UDP relay; ordinary frame boundary",
                    "scope":"online Cooperative: C0 campaign in the room's options, the first stage won on both, BC6 Continue, the kept room on the next stage and its loading over the relay; bounded runtime validation, not native parity",
                    "process_id":std::process::id(),"startup":self.startup,
                    "online":online.cooperative_receipt(),"cooperative":run.receipt(state)?}));
            }
            return Ok(json!({"target":format!("{:?}",self.target),
                "service_boundary":"two independent App processes; real UDP/global/private queues; ordinary frame boundary",
                "scope":"LAN Cooperative: C0 campaign, B6 Observe by both, BC6 records and the retried mission's loading over real UDP; bounded runtime validation, not native parity",
                "process_id":std::process::id(),"startup":self.startup,"route":self.route,
                "cooperative":run.receipt(state)?}));
        }
        if let Some(online) = &self.rounds {
            return online.receipt(state);
        }
        self.round_receipt(state)
    }

    fn round_receipt(&self, state: &AppState) -> Result<Value> {
        ensure!(self.ready(), "live LAN capture has not finished");
        self.observed_receipt(state)
    }

    fn observed_receipt(&self, state: &AppState) -> Result<Value> {
        self.observed_receipt_through(state, None)
    }

    fn observed_receipt_through(&self, state: &AppState, through: Option<u32>) -> Result<Value> {
        let checksums = if let Some(frame) = through {
            self.checksums
                .get(..frame as usize)
                .context("uncommitted checkpoint prefix")?
        } else {
            &self.checksums
        };
        let hashes = if let Some(frame) = through {
            recorded_simulation_hashes(state, Some(u64::from(frame)))?
        } else {
            self.simulation_hashes.clone()
        };
        ensure!(
            !hashes.is_empty(),
            "live capture has no recorded simulation hashes"
        );
        let sim = &state
            .match_state
            .sim_runtime
            .as_ref()
            .context("simulation")?
            .simulation;
        let live = state.match_state.live_network.as_ref();
        ensure!(
            live.is_some()
                || self
                    .failure
                    .as_ref()
                    .is_some_and(|failure| failure.returned()),
            "finished live capture lost its transport"
        );
        let mut receipt = json!({"target":format!("{:?}",self.target),"service_boundary":if self.ai.is_some() {"one App process; real HTTPS/UDP relay; ordinary frame boundary"} else if self.target.online() {"independent App processes; real HTTPS/UDP relay; ordinary frame boundary"} else {"two independent App processes; real UDP/global/private queues; ordinary frame boundary"},
            "scope":if self.target.desync_failure() { "real300-frame CRC and strong-hash prefix followed by external scoped FRAMEINFO corruption, actual checksum CE Stop and ordinary LAN lobby cleanup" } else if self.failure.is_some() { "shared 300-frame prefix followed by real peer disappearance and ordinary wait recovery or Quit; bounded runtime validation, not whole-match/native parity" } else if self.target.online() {"observed production match before real outcome; tactical commands and simulation checksums"} else if self.battle.is_some() { "Battle opening, paid GI rally/combat, paid GAPILE repair and sale with an unfinished Infantry queue, natural crew, refunds and cleanup, then former-foundation Move; bounded runtime validation, not whole-match/native parity" } else { "300 native frames and local Move/Stop; bounded runtime validation, not whole-match/native parity" },
            "process_id":std::process::id(),
            "loading_progress":crate::app::diagnostics::shell_capture::loading_runtime::progress_receipt(state)?,
            "participant_count":self.startup.as_ref().and_then(|startup|startup["sources"].as_array()).map_or(self.expected_players,Vec::len),
            "configured_participant_count":self.expected_players,"active_participant_count":live.map(|live|live.peers().len()+1),"cpu_count":usize::from(self.ai.is_some()),
            "active_mobile_count":sim.entities().values_sorted().filter(|entity|entity.is_alive() && !entity.in_limbo() && matches!(entity.category(),crate::map::entities::EntityCategory::Infantry | crate::map::entities::EntityCategory::Unit)).count(),
            "required_frames":through.unwrap_or(self.required_frames()),"battle":self.battle.as_ref().map(|battle| battle.receipt(state)).transpose()?,
            "failure":self.failure.as_ref().map(|failure| failure.receipt()),"mass_input":self.mass.as_ref().map(mass::MassInput::receipt),
            "ai_activity":self.ai.as_ref().map(AiActivity::receipt).transpose()?,
            "solo_ai_input_bound_native_frames":self.ai.as_ref().map(|_| SOLO_AI_INPUT_BOUND_FRAMES),
            "production_broadcast":self.config.as_ref().map(|config|matches!(config.transport,LiveLanTransport::ProductionBroadcast)),
            "bind":self.config.as_ref().and_then(|config|config.transport.explicit_endpoints()).map(|(bind,_)|bind.to_string()),"peer":self.config.as_ref().and_then(|config|config.transport.explicit_endpoints()).map(|(_,peer)|peer.to_string()),"startup":self.startup,"map_admission":self.map_admission,
            "native_checksums":checksums,"simulation_hashes":hashes,
            "simulation_hash_interval":crate::sim::replay::PLAY_HASH_INTERVAL.get(),
            "actions":self.actions,"actor_last_seen":self.actor_last_seen,"observed_movement":self.observed_movement,"stopped_position":self.stop_observation.position(),"stop_settled_at_frame":self.stop_observation.settled.as_ref().map(|(frame,_)|frame),"route":self.route,
            "final_native_frame":sim.session().binary_frame(),"checksum_failure":format!("{:?}",sim.pending_network_failure()),
            "sent_commands":live.map(|live| live.sent_commands()),"peers":live.map(|live| live.peers().iter().map(|peer| json!({"house":peer.house_id(),"frame":peer.frame(),"advertised_commands":peer.advertised_commands(),"received_commands":peer.received_commands()})).collect::<Vec<_>>()),
            "mobile_objects":sim.entities().values_sorted().filter(|entity| matches!(entity.category(), crate::map::entities::EntityCategory::Infantry | crate::map::entities::EntityCategory::Unit)).map(|entity| json!({"id":entity.stable_id(),"owner":sim.interner().resolve(entity.owner()),"position":entity.position,"alive":entity.is_alive()})).collect::<Vec<_>>() });
        receipt["latency"] = json!(self.latency.as_ref().map(latency::LatencyCapture::receipt));
        Ok(receipt)
    }
}

/// Reuse the rendered hitbox and ordinary pointer handlers for both network
/// entry routes; never inject a main-menu result from the capture driver.
fn click_main_menu(
    state: &mut AppState,
    control: crate::ui::main_menu_shell::MainMenuControlId,
) -> Result<()> {
    let layout = crate::app::frontend::shell_layout::main_menu_layout(state);
    let rect = layout
        .buttons
        .iter()
        .find(|button| button.id == control)
        .context("rendered multiplayer main-menu button")?
        .rect;
    set_window_pointer_to_rect_center(state, rect);
    App::handle_main_menu_shell_mouse_move(state);
    App::handle_main_menu_shell_mouse_down(state);
    App::handle_main_menu_shell_mouse_up(state);
    MoviesCapture::restore_neutral_pointer(state);
    Ok(())
}

/// Read committed observations from the one diagnostic replay owner. This
/// capture neither computes another hash nor admits command journals as state.
fn recorded_simulation_hashes(
    state: &AppState,
    through_tick: Option<u64>,
) -> Result<Vec<(u64, u64)>> {
    let replay = state
        .match_state
        .match_diagnostics
        .replay_log
        .as_ref()
        .context("running match diagnostic replay")?;
    let hashes: Vec<_> = replay
        .ticks
        .iter()
        .filter(|tick| through_tick.is_none_or(|limit| tick.tick <= limit))
        .filter_map(|tick| tick.state_hash.map(|hash| (tick.tick, hash)))
        .collect();
    ensure!(!hashes.is_empty(), "diagnostic replay has no state hashes");
    Ok(hashes)
}

fn terminal_simulation_receipt(state: &AppState) -> Result<Value> {
    let sim = &state
        .match_state
        .sim_runtime
        .as_ref()
        .context("terminal simulation")?
        .simulation;
    ensure!(
        sim.terminal_score_snapshot().is_some(),
        "multiplayer result has no terminal score snapshot"
    );
    let hashes = recorded_simulation_hashes(state, None)?;
    let (tick, hash) = hashes.last().copied().context("terminal replay hash")?;
    ensure!(
        tick == sim.session().tick(),
        "result lacks the terminal frame's recorded simulation hash"
    );
    Ok(
        json!({"tick":tick,"native_frame":sim.session().binary_frame(),
        "hash":hash,"recorded_hashes":hashes,"score_finalized":true,
        "state_hash_owners":sim.state_hash_owners()}),
    )
}

fn vera_cooperative_mode_id(state: &AppState) -> Result<i32> {
    state
        .frontend
        .skirmish_modes
        .iter()
        .find(|mode| mode.class == crate::skirmish_modes::MpModeClass::VeraCooperative)
        .map(|mode| mode.id)
        .context("VERA Cooperative mode")
}

/// The frozen role/House grant matrix both peers received from the start
/// roster and the external campaign overlay. It is read through
/// ScenarioSession's public grant queries, never rebuilt here.
fn vera_coop_grants(state: &AppState) -> Result<Value> {
    let sim = &state
        .match_state
        .sim_runtime
        .as_ref()
        .context("VERA Coop simulation")?
        .simulation;
    let session = sim.session();
    ensure!(
        session.has_network_house_grants(),
        "VERA Coop mission has no shared House grant table"
    );
    let houses: Vec<String> = sim
        .houses()
        .order()
        .iter()
        .map(|owner| sim.interner().resolve(*owner).to_owned())
        .collect();
    let mut grants = Vec::new();
    for (source, primary) in session.network_sources(houses.len()).collect::<Vec<_>>() {
        let controlled: Vec<String> = houses
            .iter()
            .enumerate()
            .filter(|(house, _)| session.network_source_controls_house(source, *house))
            .map(|(_, name)| name.clone())
            .collect();
        grants.push(json!({
            "source": source,
            "primary": houses.get(primary),
            "controlled": controlled,
        }));
    }
    let current = session.current_house();
    let local_primary = current.and_then(|owner| {
        sim.houses()
            .order()
            .iter()
            .position(|candidate| *candidate == owner)
    });
    Ok(json!({
        "houses": houses,
        "shared_house_sources": session.shares_houses(),
        "current_house": current.map(|owner| sim.interner().resolve(owner)),
        "local_primary": local_primary,
        "grants": grants,
    }))
}

/// Prove the peer's primary House is outside the local command authority:
/// the grant matrix must exclude it both ways, and the production pick and
/// selection owners must not select one of its mobile objects.
fn vera_coop_probe(state: &mut AppState) -> Result<Value> {
    let (local_source, local_primary, peer_source, peer_primary, peer_entity) = {
        let sim = &state
            .match_state
            .sim_runtime
            .as_ref()
            .context("VERA Coop probe simulation")?
            .simulation;
        let session = sim.session();
        ensure!(
            session.has_network_house_grants(),
            "VERA Coop mission has no shared House grant table"
        );
        let houses: Vec<String> = sim
            .houses()
            .order()
            .iter()
            .map(|owner| sim.interner().resolve(*owner).to_owned())
            .collect();
        let current = session.current_house().context("probe current House")?;
        let local_primary = sim
            .houses()
            .order()
            .iter()
            .position(|candidate| *candidate == current)
            .context("probe current House index")?;
        let sources: Vec<(i8, usize)> = session.network_sources(houses.len()).collect();
        let local_source = sources
            .iter()
            .find(|(_, primary)| *primary == local_primary)
            .map(|(source, _)| *source)
            .context("probe local command source")?;
        ensure!(
            session.network_source_controls_house(local_source, local_primary),
            "local command source does not control its own primary House"
        );
        for &(source, primary) in &sources {
            if primary == local_primary {
                continue;
            }
            ensure!(
                !session.network_source_controls_house(local_source, primary),
                "local command source controls another role's primary House {primary}"
            );
            ensure!(
                !session.network_source_controls_house(source, local_primary),
                "peer command source {source} controls the local primary House {local_primary}"
            );
        }
        let (peer_source, peer_primary) = sources
            .iter()
            .copied()
            .find(|(source, primary)| *source != local_source && *primary != local_primary)
            .context("probe peer primary House")?;
        let peer_owner = sim.houses().order()[peer_primary];
        let peer_entity = sim
            .entities()
            .values_sorted()
            .filter(|entity| {
                entity.owner() == peer_owner
                    && entity.is_alive()
                    && !entity.in_limbo()
                    && matches!(
                        entity.category(),
                        crate::map::entities::EntityCategory::Unit
                            | crate::map::entities::EntityCategory::Infantry
                    )
            })
            .map(|entity| entity.stable_id())
            .next()
            .with_context(|| {
                format!(
                    "peer primary House {} has no mobile object to probe",
                    houses[peer_primary]
                )
            })?;
        (
            local_source,
            local_primary,
            peer_source,
            peer_primary,
            peer_entity,
        )
    };
    let (picked, selection) = battle::selection_after_clicking(state, peer_entity)?;
    ensure!(
        picked == Some(peer_entity),
        "peer-House selection probe missed object {peer_entity}: picked={picked:?}, selection={selection:?}"
    );
    ensure!(
        !selection.contains(&peer_entity),
        "peer-granted House object {peer_entity} entered the local selection {selection:?}"
    );
    Ok(json!({
        "local_source": local_source,
        "local_primary": local_primary,
        "peer_source": peer_source,
        "peer_primary": peer_primary,
        "peer_object": peer_entity,
        "peer_object_picked": picked,
        "selection_after_peer_click": selection,
        "selection_empty": selection.is_empty(),
    }))
}

/// The running mission's VERA Coop identity plus the grants and the probe.
fn vera_coop_runtime_receipt(state: &AppState, probe: Option<&Value>) -> Result<Value> {
    let session = &state.match_state.session;
    let setup = session
        .network_setup()
        .context("VERA Coop runtime roster")?;
    let config = session.network_vera_coop_map_config();
    let humans: Vec<Value> = setup
        .humans
        .iter()
        .map(|human| {
            json!({
                "name": human.name,
                "raw_slot_index": human.raw_slot_index,
                "role": human.role.native_value(),
                "observer": human.is_observer(),
            })
        })
        .collect();
    Ok(json!({
        "map_file": setup.selected_map_file,
        "local_player_index": setup.local_player_index,
        "host": session.host(),
        "campaign_directory": config.and_then(|config| config.campaign_directory.clone()),
        "campaign_sha256": config.and_then(|config| config.campaign_sha256.clone()),
        "config_sha256": config.map(|config| config.config_sha256.clone()),
        "humans": humans,
        "grants": vera_coop_grants(state)?,
        "control_probe": probe,
    }))
}

/// Error-only snapshot for the new VERA Coop setup targets. It reads current
/// owners without sending requests, accepting the map, or writing a receipt.
fn vera_coop_setup_watchdog_snapshot(
    capture: &LiveLanCapture,
    state: &AppState,
    rendered: PresentedShell,
) -> Value {
    let session = &state.match_state.session;
    let launch = session.network_setup();
    let participants = launch.map(|launch| {
        launch
            .humans
            .iter()
            .enumerate()
            .map(|(index, human)| {
                json!({
                    "name": human.name,
                    "raw_row": human.raw_slot_index,
                    "role": human.role.native_value(),
                    "ready": session.network_player_ready(index),
                    "map_status": session.network_map_status(index),
                })
            })
            .collect::<Vec<_>>()
    });
    let metadata = session.network_map_metadata();
    let config = session.network_vera_coop_map_config();
    let source = session.network_local_map_source();
    let protocol = state.frontend.network_setup_protocol.as_ref();
    let setup_view = setup::view(state);
    let lobby = state.frontend.lan.as_ref();

    json!({
        "target": format!("{:?}", capture.target),
        "capture_phase": format!("{:?}", capture.phase),
        "screen": format!("{:?}", state.frontend.screen),
        "presented_shell": format!("{rendered:?}"),
        "host": session.host(),
        "network_setup_present": launch.is_some(),
        "mode_id": launch.map(|launch| launch.mode.id),
        "selected_map": launch.and_then(|launch| launch.selected_map_file.as_deref()),
        "map_metadata": metadata.map(|metadata| json!({
            "byte_length": metadata.byte_length,
            "source_sha256": metadata.source_sha256,
            "effective_digest": metadata.effective_digest,
            "digest": metadata.digest,
            "player_capacity": metadata.player_capacity,
        })),
        "local_map_source_identity": source.and_then(|source| source.physical_identity()),
        "coop_config": config.map(|config| json!({
            "map_file": config.map_file,
            "source_sha256": config.source_sha256,
            "config_sha256": config.config_sha256,
        })),
        "participants": participants,
        "protocol_present": protocol.is_some(),
        "pending_setup_requests": protocol.map(|protocol| protocol.diagnostic_pending_requests()),
        "protocol_accept_eligible": protocol.map(|protocol| protocol.diagnostic_accept_eligible()),
        "accept_enabled": setup_view.as_ref().map(|view| view.accept_enabled),
        "start_requested": setup_view.as_ref().map(|view| view.start_requested),
        "lan_lobby": lobby.map(|runtime| json!({
            "pending_game": runtime.pending_game(),
            "modal": runtime.modal,
            "games": runtime.lobby.games.iter().map(|game| json!({
                "name": game.name,
                "open": game.open,
            })).collect::<Vec<_>>(),
        })),
    })
}

fn coop_setup_rows(
    launch: &crate::skirmish_launch::SkirmishLaunchSession,
) -> Result<(usize, usize)> {
    ensure!(
        launch.humans.len() == 2
            && launch
                .humans
                .iter()
                .all(|human| human.raw_slot_index.is_some()),
        "VERA Coop setup capture requires exactly two admitted human rows"
    );
    let host = launch
        .humans
        .iter()
        .position(|human| human.name == HOST_NAME)
        .context("admitted LAN host row")?;
    let guest = launch
        .humans
        .iter()
        .position(|human| human.name == GUEST_NAME)
        .context("admitted LAN guest row")?;
    ensure!(host != guest, "host and guest rows alias");
    Ok((host, guest))
}

fn vera_coop_setup_snapshot(state: &AppState) -> Result<Value> {
    let session = &state.match_state.session;
    let launch = session
        .network_setup()
        .context("VERA Coop setup snapshot")?;
    ensure!(
        launch.mode.class == crate::skirmish_modes::MpModeClass::VeraCooperative,
        "setup snapshot is not VERA Coop"
    );
    let file = launch
        .selected_map_file
        .as_deref()
        .context("selected VERA Coop map")?;
    let metadata = session
        .network_map_metadata()
        .context("accepted VERA Coop map metadata")?;
    // VERA Coop LAN exchanges only the map/config identity in HostOptions: the
    // authored file is preinstalled on both peers, so the ordinary map
    // transfer never binds a local source. In that case the accepted host
    // metadata carries the same physical identity, and the frozen loader
    // re-verifies the actual bytes against the campaign config before start.
    let source = session.network_local_map_source();
    let (byte_length, physical_sha256, effective_digest) = match source {
        Some(source) => {
            let (byte_length, physical_sha256) = source
                .physical_identity()
                .context("physical VERA Coop map identity")?;
            let effective_digest = session
                .network_local_map_effective_digest()
                .context("verified local VERA Coop effective digest")?;
            (
                byte_length,
                physical_sha256.to_owned(),
                effective_digest.to_owned(),
            )
        }
        None => (
            usize::try_from(metadata.byte_length)?,
            metadata.source_sha256.clone(),
            metadata.effective_digest.clone(),
        ),
    };
    let config = session
        .network_vera_coop_map_config()
        .context("accepted VERA Coop map config")?;
    config.validate_binding(file, &metadata.source_sha256)?;
    ensure!(
        byte_length == usize::try_from(metadata.byte_length)?
            && physical_sha256 == metadata.source_sha256
            && effective_digest == metadata.effective_digest
            && metadata.source_sha256 == config.source_sha256,
        "local VERA Coop bytes or effective map differ from the accepted host identity"
    );
    let (host, guest) = coop_setup_rows(launch)?;
    let participants = launch
        .humans
        .iter()
        .enumerate()
        .map(|(index, human)| {
            json!({
                "name": human.name,
                "raw_row": human.raw_slot_index,
                "role": human.role.native_value(),
                "ready": session.network_player_ready(index),
                "map_status": session.network_map_status(index),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "host": state.match_state.session.host(),
        "mode_id": launch.mode.id,
        "map_file": file,
        "map_source_sha256": metadata.source_sha256,
        "physical_source_sha256": physical_sha256,
        "bound_local_source": source.is_some(),
        "byte_length": byte_length,
        "effective_digest": effective_digest,
        "config": {
            "map_file": config.map_file,
            "source_sha256": config.source_sha256,
            "config_sha256": config.config_sha256,
        },
        "host_row": host,
        "guest_row": guest,
        "participants": participants,
    }))
}

fn ensure_coop_setup_ready_transition(
    initial: &Value,
    final_state: &Value,
    host_observes_room: bool,
) -> Result<()> {
    for field in ["map_file", "map_source_sha256", "effective_digest"] {
        ensure!(
            initial[field] == final_state[field],
            "VERA Coop map identity changed while accepting the guest: {field}"
        );
    }
    ensure!(
        initial["config"]["config_sha256"] == final_state["config"]["config_sha256"],
        "VERA Coop config changed during the Ready transition"
    );
    let initial_host = coop_setup_participant(initial, HOST_NAME)?;
    let initial_guest = coop_setup_participant(initial, GUEST_NAME)?;
    let final_host = coop_setup_participant(final_state, HOST_NAME)?;
    let final_guest = coop_setup_participant(final_state, GUEST_NAME)?;
    if host_observes_room {
        ensure!(
            initial_host["ready"] == true
                && initial_host["map_status"] == 1
                && final_host["ready"] == true
                && final_guest["ready"] == true
                && final_host["map_status"] == 1
                && final_guest["map_status"] == 1,
            "host did not observe both real LAN participants Ready with accepted maps"
        );
    } else {
        ensure!(
            initial_guest["ready"] == false
                && initial_guest["map_status"] == 1
                && final_guest["ready"] == true
                && final_guest["map_status"] == 1,
            "guest local Ready/map-status did not transition across production Accept"
        );
    }
    for name in [HOST_NAME, GUEST_NAME] {
        let before = coop_setup_participant(initial, name)?;
        let after = coop_setup_participant(final_state, name)?;
        ensure!(
            before["raw_row"] == after["raw_row"] && before["role"] == after["role"],
            "VERA Coop participant role changed while accepting: {name}"
        );
    }
    Ok(())
}

fn coop_setup_participant<'a>(snapshot: &'a Value, name: &str) -> Result<&'a Value> {
    snapshot["participants"]
        .as_array()
        .context("VERA Coop participant snapshot")?
        .iter()
        .find(|row| row["name"] == name)
        .with_context(|| format!("VERA Coop {name} participant"))
}

fn local_owner(state: &AppState) -> Result<String> {
    let launch = state
        .match_state
        .session
        .network_setup()
        .context("network setup")?;
    crate::sim::scenario_bootstrap::normalized_launch_slots(launch)
        .into_iter()
        .find(|slot| slot.is_controlled_by(launch.local_player_index))
        .map(|slot| slot.owner_name)
        .context("local normalized House")
}

fn startup_receipt(state: &AppState) -> Result<Value> {
    let launch = state
        .match_state
        .session
        .network_setup()
        .context("network setup")?;
    let sim = &state
        .match_state
        .sim_runtime
        .as_ref()
        .context("simulation")?
        .simulation;
    crate::app::diagnostics::shell_capture::network_startup_receipt(state, launch, Some(sim))
}
