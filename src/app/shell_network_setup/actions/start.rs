//! Host command 588's ordered validation and commit: BC (005DCF00..5DD5DB)
//! ends in GO, C2 (0x7905D0..0x790BEB) in a StartGame request. GO and the
//! server's OnGameStart complete later, never the Start click.

use super::*;
use crate::assets::csf_file::CsfArg;
use crate::skirmish_modes::ModeStartEffect;

use super::super::chat_notice as message;

pub(super) fn request_start(state: &mut AppState) -> anyhow::Result<()> {
    let wol = super::super::dialog(state).wol();
    anyhow::ensure!(
        state.match_state.session.host(),
        "only the host starts a network game"
    );
    anyhow::ensure!(
        state
            .frontend
            .network_setup_protocol
            .as_ref()
            .is_some_and(|protocol| !protocol.locked),
        "setup is locked"
    );
    // C2 ignores Start while its StartGame request is pending (00B77DEC).
    if state
        .frontend
        .network_setup_protocol
        .as_ref()
        .is_some_and(|protocol| protocol.start_in_progress())
    {
        return Ok(());
    }
    super::super::remember_setup(state);
    let launch = state
        .match_state
        .session
        .network_setup()
        .ok_or_else(|| anyhow::anyhow!("no setup"))?
        .clone();
    let humans = launch
        .humans
        .iter()
        .filter(|human| !human.is_observer())
        .count() as i32;
    if !has_minimum_players(&launch) {
        message(state, "TXT_ONLY_ONE", &[]);
        return Ok(());
    }
    // C2 0x790698: WOL forbids AI players in a game with observers.
    if wol && humans < launch.humans.len() as i32 && launch.ai_player_count != 0 {
        message(state, "TXT_NO_AI_OBSERVATION", &[]);
        return Ok(());
    }
    let mut missing_map = false;
    for index in 0..launch.humans.len() {
        if index != launch.local_player_index
            && state
                .match_state
                .session
                .network_map_status(index)
                .unwrap_or(0)
                == 0
        {
            let map_name = super::super::map_display_name(state)
                .unwrap_or_default()
                .to_owned();
            message(
                state,
                "GUI:HostNoMap",
                &[
                    CsfArg::Str(&launch.humans[index].name),
                    CsfArg::Str(&map_name),
                ],
            );
            missing_map = true;
        }
    }
    if missing_map {
        return Ok(());
    }
    let mode = crate::skirmish_modes::mode_by_id(&state.frontend.skirmish_modes, launch.mode.id)
        .ok_or_else(|| anyhow::anyhow!("unknown mode"))?
        .clone();
    let capacity = state
        .match_state
        .session
        .network_setup_capacity()
        .unwrap_or(0);
    if wol {
        // C2 0x79071E..0x7908D5: allies, readiness, then capacity. VERA's
        // explicit AI rows share the map with human players, even though
        // only humans belong to the network/relay roster.
        if all_allied(state) {
            message(state, "TXT_CANNOT_ALLY", &[]);
            return Ok(());
        }
        if !all_ready(state) {
            return Ok(());
        }
        if capacity < humans + launch.opponents.len() as i32 {
            message(
                state,
                "TXT_SCENARIO_TOO_SMALL",
                &[CsfArg::Int(i64::from(capacity))],
            );
            return Ok(());
        }
        // VERA online/LAN clients use the shared thirty-player admission
        // owner; the original C2 six-user service limit does not apply.
        if launch.humans.len() > crate::skirmish_launch::NETWORK_PLAYER_SLOT_COUNT {
            message(state, "TXT_TOO_MANY_PLAYERS", &[]);
            return Ok(());
        }
        if !mode_start(state, &mode)? {
            return Ok(());
        }
        // 0x79096A..0x790B18: a clan room starts only with two squads of
        // equal size; the observer row's node is not counted.
        if state.match_state.session.network_room().clan {
            let observer = state
                .match_state
                .session
                .network_observer_row()
                .and_then(|row| {
                    launch
                        .humans
                        .iter()
                        .find(|human| human.raw_slot_index == Some(row) && human.is_observer())
                })
                .map(|human| human.name.clone());
            if !crate::app::shell_wol::lobby::clan_teams_even(state, observer.as_deref()) {
                message(state, "TXT_CLAN_NOSTART", &[]);
                return Ok(());
            }
        }
    } else {
        let capacity_ok = capacity >= humans + launch.ai_player_count;
        if !capacity_ok {
            message(
                state,
                "TXT_SCENARIO_TOO_SMALL",
                &[CsfArg::Int(i64::from(capacity))],
            );
        }
        // The virtual14 callback still runs after a failed capacity check
        // and may mutate roles or the retained Cooperative partner record.
        let mode_ok = mode_start(state, &mode)?;
        if !capacity_ok || !mode_ok {
            return Ok(());
        }
        if all_allied(state) {
            message(state, "TXT_CANNOT_ALLY", &[]);
            return Ok(());
        }
        if !all_ready(state) {
            return Ok(());
        }
    }
    let launch = state
        .match_state
        .session
        .network_setup()
        .expect("validated setup")
        .clone();
    if !wol {
        crate::app::lan_service::prepare_host_timing(state)?;
    }
    let max_ahead_frames = if wol {
        // Start Game Now derives WOL timing after OnGameStart.
        None
    } else {
        Some(
            state
                .match_state
                .session
                .network_max_ahead_frames()
                .ok_or_else(|| anyhow::anyhow!("host start has no admitted LAN protocol timing"))?,
        )
    };
    // mode+68 before the sole 69B8C0 assignment owner (BC 5DD3BD, C2
    // 0x790B38). The callback's declared-count write can survive a false
    // result while existing AI rows remain, so commit its two effects
    // independently.
    let map = state
        .frontend
        .scenario_catalog
        .shell_maps()
        .iter()
        .find(|map| {
            launch
                .selected_map_file
                .as_deref()
                .is_some_and(|file| file.eq_ignore_ascii_case(&map.file_name))
        })
        .ok_or_else(|| anyhow::anyhow!("selected scenario is unavailable"))?;
    let preparation = state
        .frontend
        .multiplayer_shell_runtime
        .prepare_network_mode_start(&launch, map)?;
    state
        .match_state
        .session
        .apply_network_ai_preparation(preparation.declared_count, preparation.opponents)?;
    let launch = state
        .match_state
        .session
        .network_setup()
        .expect("mode callback retains setup")
        .clone();
    let banned =
        crate::app::shell_network_setup::bans::banned_countries(state, launch.options.bans);
    let resolved = state
        .frontend
        .multiplayer_shell_runtime
        .resolve_network_assignments(&launch, &banned)?;
    state
        .match_state
        .session
        .accept_network_assignments(resolved)?;
    if wol {
        // C2: slot names 77DB90 before options 5E32D0(1,1).
        queue(state, NetworkSetupRequest::PublishRoster);
        queue(state, NetworkSetupRequest::PublishOptions);
    } else {
        queue(state, NetworkSetupRequest::PublishOptions);
        queue(state, NetworkSetupRequest::PublishRoster);
    }
    // Native mode+64 follows the resolved options/name publication, before
    // preview disposal and GO. Only Cooperative consumes Scenario RNG here.
    // C2 then calls mode+0x24 (0x790B62), a no-op except MPCooperative's
    // 5C1CB0 flag for its later 5C2680 save step: not ported.
    let launch = state
        .match_state
        .session
        .network_setup()
        .expect("resolved setup");
    state
        .frontend
        .multiplayer_shell_runtime
        .complete_network_mode_assignments(launch)?;
    let users: Vec<String> = launch
        .humans
        .iter()
        .map(|human| human.name.clone())
        .collect();
    if launch.mode.class.uses_cooperative_setup() {
        let rounds = state
            .frontend
            .multiplayer_shell_runtime
            .cooperative_round_assignments()
            .to_vec();
        queue(
            state,
            NetworkSetupRequest::PublishCooperativeAssignments(rounds),
        );
    }
    let Some(max_ahead_frames) = max_ahead_frames else {
        // C2 0x790BB9: API+0x48 StartGame with every channel user, then
        // 00B77E28/00B77DEC until OnGameStart answers.
        queue(state, NetworkSetupRequest::StartGame { users });
        if let Some(protocol) = state.frontend.network_setup_protocol.as_mut() {
            protocol.wol_start_pending = true;
        }
        return Ok(());
    };
    let agreed_seed = state
        .match_state
        .session
        .network_start_seed()
        .ok_or_else(|| anyhow::anyhow!("host setup has no proposed seed"))?;
    queue(
        state,
        NetworkSetupRequest::Go {
            agreed_seed,
            max_ahead_frames,
        },
    );
    let go_pending = state
        .match_state
        .session
        .network_setup()
        .map_or_else(Vec::new, |launch| {
            (0..launch.humans.len())
                .filter(|source| *source != launch.local_player_index)
                .collect()
        });
    if let Some(protocol) = state.frontend.network_setup_protocol.as_mut() {
        protocol.locked = true;
        // The transport starts the native drain only after all GO packets
        // have entered its existing global send queue.
        protocol.lan_go = Some(super::super::LanGoWait::HostSending {
            pending: go_pending,
        });
        // Native's offer blocks the dialog until it ends, so Start cannot
        // follow a pending one; GO drops it.
        protocol.drop_preview_offer();
    }
    state.frontend.skirmish_preview_texture = None;
    Ok(())
}

/// A direct spawn host's start (docs/spawn.md): the launcher already
/// validated the roster, teams and map, so only BC's tail runs: LAN start
/// timing, then GO to every admitted guest. GO's replies, drain and
/// completion are the ordinary LAN owners'.
pub(crate) fn begin_external_lan_go(state: &mut AppState) -> anyhow::Result<()> {
    anyhow::ensure!(
        super::super::external(state) && !super::super::dialog(state).wol(),
        "no direct spawn setup"
    );
    anyhow::ensure!(
        state.match_state.session.host(),
        "only the direct host sends GO"
    );
    anyhow::ensure!(
        state
            .frontend
            .network_setup_protocol
            .as_ref()
            .is_some_and(|protocol| !protocol.locked),
        "setup is locked"
    );
    crate::app::lan_service::prepare_host_timing(state)?;
    let max_ahead_frames = state
        .match_state
        .session
        .network_max_ahead_frames()
        .ok_or_else(|| anyhow::anyhow!("host start has no admitted LAN protocol timing"))?;
    let agreed_seed = state
        .match_state
        .session
        .network_start_seed()
        .ok_or_else(|| anyhow::anyhow!("direct setup has no seed"))?;
    let launch = state
        .match_state
        .session
        .network_setup()
        .ok_or_else(|| anyhow::anyhow!("no setup"))?;
    let go_pending = (0..launch.humans.len())
        .filter(|source| *source != launch.local_player_index)
        .collect();
    log::info!(
        "Direct spawn: every guest answered; GO with MaxAhead {max_ahead_frames}, seed {agreed_seed}"
    );
    queue(
        state,
        NetworkSetupRequest::Go {
            agreed_seed,
            max_ahead_frames,
        },
    );
    let protocol = state
        .frontend
        .network_setup_protocol
        .as_mut()
        .expect("checked setup");
    protocol.locked = true;
    protocol.lan_go = Some(super::super::LanGoWait::HostSending {
        pending: go_pending,
    });
    Ok(())
}

/// VERA's shared LAN/online admission allows a human to play the ordinary
/// skirmish AI without creating another network participant. Observers and
/// a stale declared AI count cannot stand in for an actual opponent slot,
/// and players sharing a House count once.
fn has_minimum_players(launch: &SkirmishLaunchSession) -> bool {
    let houses = (0..launch.humans.len())
        .filter(|&index| !launch.humans[index].is_observer() && launch.house_leader(index) == index)
        .count();
    houses >= 1 && houses + launch.opponents.len() >= 2
}

fn all_allied(state: &AppState) -> bool {
    state
        .match_state
        .session
        .network_setup()
        .expect("validated setup")
        .all_allied()
}

/// Readiness (BC; C2 0x7907E0 from the WOL user records): an unready node
/// asks every peer to accept and prints the host line.
fn all_ready(state: &mut AppState) -> bool {
    let count = state
        .match_state
        .session
        .network_setup()
        .expect("validated setup")
        .humans
        .len();
    if (0..count).all(|index| state.match_state.session.network_player_ready(index)) {
        return true;
    }
    queue(state, NetworkSetupRequest::RequestAcceptance);
    message(state, "GUI:HostGameStartHost", &[]);
    false
}

/// mode+14 (BC and C2 0x79091D): its effect commits, an error key prints.
fn mode_start(
    state: &mut AppState,
    mode: &crate::skirmish_modes::SkirmishGameMode,
) -> anyhow::Result<bool> {
    let launch = state
        .match_state
        .session
        .network_setup()
        .expect("validated setup")
        .clone();
    if mode.class.is_vera_cooperative() {
        let Some(file) = launch.selected_map_file.as_deref() else {
            message(state, "VERA Coop requires a selected map.", &[]);
            return Ok(false);
        };
        if state.match_state.session_mode() != crate::app::match_runtime::session::SessionMode::Lan
        {
            message(
                state,
                "VERA Coop map configuration sync is currently LAN-only.",
                &[],
            );
            return Ok(false);
        }
        let Some(config) = state
            .match_state
            .session
            .network_vera_coop_map_config()
            .cloned()
        else {
            message(
                state,
                "VERA Coop host options omit the selected map configuration.",
                &[],
            );
            return Ok(false);
        };
        let map = (|| -> anyhow::Result<crate::map::source::LoadedMapIni> {
            let root = &state
                .platform
                .game_config
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("no asset directory"))?
                .paths
                .ra2_dir;
            let assets = state
                .process_assets
                .manager()
                .ok_or_else(|| anyhow::anyhow!("assets missing"))?;
            crate::map::source::load_network_map_ini(root, file, assets)
        })();
        let valid = match map {
            Ok(map) => match state
                .match_state
                .session
                .network_start_seed()
                .ok_or_else(|| anyhow::anyhow!("VERA Coop start has no accepted shared seed"))
                .and_then(|seed| {
                    let (_, source_sha256) = map
                        .source
                        .physical_identity()
                        .ok_or_else(|| anyhow::anyhow!("VERA Coop map has no physical identity"))?;
                    crate::app::match_bootstrap::resolve_vera_cooperative_houses_with_config(
                        &map.ini,
                        file,
                        source_sha256,
                        &config,
                        &launch,
                        seed,
                    )
                }) {
                Ok(assignments) => {
                    // The map, role and House configuration is valid, so the
                    // start proceeds to GO. The accepted start freezes these
                    // assignments and the loader installs the shared House
                    // command grants from the same effective map.
                    log::info!(
                        "VERA Coop role roster resolves to {} player Houses",
                        assignments.participants.len()
                    );
                    true
                }
                Err(error) => {
                    log::warn!("VERA Coop map role configuration is invalid: {error}");
                    message(
                        state,
                        "VERA Coop requires Basic Player entries that name distinct Houses in the selected map.",
                        &[],
                    );
                    false
                }
            },
            Err(error) => {
                log::warn!("VERA Coop could not read map role configuration: {error:#}");
                message(
                    state,
                    "VERA Coop could not read role data from the selected map.",
                    &[],
                );
                false
            }
        };
        return Ok(valid);
    }
    let roles: Vec<_> = launch
        .humans
        .iter()
        .map(|human| human.role.native_value())
        .collect();
    match mode
        .class
        .validate_setup_start(launch.options.bases, &roles)
    {
        Ok(ModeStartEffect::None) => {}
        Ok(ModeStartEffect::FreeForAllSourceRoles) => {
            for (index, role) in launch.free_for_all_source_roles() {
                state
                    .match_state
                    .session
                    .accept_network_human_role(index, role)?;
            }
        }
        Ok(ModeStartEffect::CooperativePartners) => {
            state
                .frontend
                .multiplayer_shell_runtime
                .bind_network_cooperative_partners(&launch);
        }
        Err(key) => {
            if let Some(key) = key {
                message(state, key, &[]);
            }
            return Ok(false);
        }
    }
    Ok(true)
}

/// Resolve the synchronized lobby roles against the selected map's authored
/// Basic Player roster. Random roles consume the remaining configured Houses
/// in a seed-stable order shared by every participant.
fn resolve_vera_cooperative_houses(
    ini: &crate::rules::ini_parser::IniFile,
    launch: &crate::skirmish_launch::SkirmishLaunchSession,
    seed: u32,
) -> Result<crate::skirmish_launch::VeraCoopHouseAssignments, String> {
    crate::app::match_bootstrap::resolve_vera_cooperative_houses(ini, launch, seed)
        .map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vera_coop_start_resolves_distinct_configured_map_houses() {
        let valid = crate::rules::ini_parser::IniFile::from_str(
            "[Basic]\nPlayer1=Alpha\nPlayer2=Beta\n[Houses]\n0=Alpha\n1=Beta\n",
        );
        let mut launch = fixture();
        assert_eq!(
            resolve_vera_cooperative_houses(&valid, &launch, 7).unwrap(),
            crate::skirmish_launch::VeraCoopHouseAssignments {
                participants: vec![crate::skirmish_launch::VeraCoopHouseAssignment {
                    participant_name: launch.humans[0].name.clone(),
                    raw_slot_index: 0,
                    player_index: 0,
                    owner_name: "Alpha".to_owned(),
                }]
            }
        );
        launch.humans[0].role = crate::skirmish_launch::LaunchHumanRole::Player { group: 1 };
        assert_eq!(
            resolve_vera_cooperative_houses(&valid, &launch, 7).unwrap(),
            crate::skirmish_launch::VeraCoopHouseAssignments {
                participants: vec![crate::skirmish_launch::VeraCoopHouseAssignment {
                    participant_name: launch.humans[0].name.clone(),
                    raw_slot_index: 0,
                    player_index: 1,
                    owner_name: "Beta".to_owned(),
                }]
            }
        );
        launch.humans[0].role = crate::skirmish_launch::LaunchHumanRole::Player { group: 0 };

        let missing = crate::rules::ini_parser::IniFile::from_str(
            "[Basic]\nPlayer1=Missing\n[Houses]\n0=Alpha\n",
        );
        assert!(resolve_vera_cooperative_houses(&missing, &launch, 7).is_err());

        let empty = crate::rules::ini_parser::IniFile::from_str("[Houses]\n0=Alpha\n");
        assert!(resolve_vera_cooperative_houses(&empty, &launch, 7).is_err());

        launch.humans[0].role = crate::skirmish_launch::LaunchHumanRole::Player { group: -2 };
        let mut second = launch.humans[0].clone();
        second.name = "Guest".to_owned();
        second.raw_slot_index = Some(1);
        launch.humans.push(second);
        let two_players = crate::rules::ini_parser::IniFile::from_str(
            "[Basic]\nPlayer1=Alpha\nPlayer2=Beta\n[Houses]\n0=Alpha\n1=Beta\n",
        );
        let first = resolve_vera_cooperative_houses(&two_players, &launch, 123).unwrap();
        let second = resolve_vera_cooperative_houses(&two_players, &launch, 123).unwrap();
        assert_eq!(first, second);
        assert_ne!(
            first.participants[0].owner_name, first.participants[1].owner_name,
            "random roles must not share a House"
        );
        let by_name = |assignments: &crate::skirmish_launch::VeraCoopHouseAssignments| {
            assignments
                .participants
                .iter()
                .map(|assignment| {
                    (
                        assignment.participant_name.clone(),
                        assignment.owner_name.clone(),
                    )
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let expected = by_name(&first);
        let mut reordered = launch.clone();
        reordered.humans.reverse();
        assert_eq!(
            by_name(&resolve_vera_cooperative_houses(&two_players, &reordered, 123).unwrap()),
            expected
        );
        let mut too_many = launch.clone();
        let mut third = too_many.humans[0].clone();
        third.name = "Third".to_owned();
        third.raw_slot_index = Some(2);
        too_many.humans.push(third);
        assert!(resolve_vera_cooperative_houses(&two_players, &too_many, 123).is_err());
    }

    #[test]
    fn vera_coop_sidecar_changes_the_resolved_house_and_is_bound_to_map_bytes() {
        let map = crate::rules::ini_parser::IniFile::from_str(
            "[Basic]\nPlayer1=Alpha\n[Houses]\n0=Alpha\n1=Beta\n[Alpha]\n[Beta]\n",
        );
        let supplemental = crate::rules::ini_parser::IniFile::from_str(
            "[Basic]\nPlayer1=Beta\n[Alpha]\nPlayer1Control=yes\n",
        );
        let config =
            crate::map::coop_config::VeraCoopMapConfig::from_ini(&map, &supplemental).unwrap();
        let snapshot = crate::app::match_bootstrap::NetworkVeraCoopMapConfig::new(
            "test.map".into(),
            "a".repeat(64),
            config,
        )
        .unwrap();
        let launch = fixture();
        let native =
            crate::app::match_bootstrap::resolve_vera_cooperative_houses(&map, &launch, 7).unwrap();
        let configured = crate::app::match_bootstrap::resolve_vera_cooperative_houses_with_config(
            &map,
            "test.map",
            &"a".repeat(64),
            &snapshot,
            &launch,
            7,
        )
        .unwrap();
        assert_eq!(native.participants[0].owner_name, "Alpha");
        assert_eq!(configured.participants[0].owner_name, "Beta");
        assert!(
            snapshot
                .validate_binding("other.map", &"a".repeat(64))
                .is_err()
        );
        assert!(
            snapshot
                .validate_binding("test.map", &"b".repeat(64))
                .is_err()
        );
        let mut tampered = snapshot;
        tampered.config_sha256 = "0".repeat(64);
        assert!(
            tampered
                .validate_binding("test.map", &"a".repeat(64))
                .is_err()
        );
    }

    #[test]
    fn vera_coop_bootstrap_installs_deterministic_shared_house_command_grants() {
        use crate::sim::scenario_session::ScenarioGameMode;

        let authored = crate::rules::ini_parser::IniFile::from_str(
            "[Basic]\nPlayer1=Alpha\nPlayer2=Beta\n[Houses]\n0=Alpha\n1=Beta\n2=Neutral\n[Alpha]\n[Beta]\n[Neutral]\n",
        );
        let external = crate::rules::ini_parser::IniFile::from_str(
            "[Neutral]\nPlayerControl=yes\nPlayer2Control=yes\n",
        );
        let config =
            crate::map::coop_config::VeraCoopMapConfig::from_ini(&authored, &external).unwrap();
        let effective = config.apply_to_map(&authored).unwrap();
        let roster = crate::map::houses::parse_house_roster(&effective, &[], None);

        let mut launch = fixture();
        launch.humans[0].raw_slot_index = Some(0);
        let mut guest = launch.humans[0].clone();
        guest.name = "Guest".into();
        guest.raw_slot_index = Some(1);
        guest.role = crate::skirmish_launch::LaunchHumanRole::Player { group: 1 };
        guest.slot.color_index = 1;
        launch.humans.push(guest);
        let assignments = crate::skirmish_launch::VeraCoopHouseAssignments {
            participants: vec![
                crate::skirmish_launch::VeraCoopHouseAssignment {
                    participant_name: launch.humans[0].name.clone(),
                    raw_slot_index: 0,
                    player_index: 0,
                    owner_name: "Alpha".into(),
                },
                crate::skirmish_launch::VeraCoopHouseAssignment {
                    participant_name: "Guest".into(),
                    raw_slot_index: 1,
                    player_index: 1,
                    owner_name: "Beta".into(),
                },
            ],
        };
        let mut sim = crate::sim::world::Simulation::new();
        sim.session_mut_for_test()
            .set_game_mode(ScenarioGameMode::Lan);
        sim.session_mut_for_test().install_network_timing(5);
        crate::sim::scenario_bootstrap::initialize_vera_cooperative_houses(
            &mut sim,
            &roster,
            &crate::rules::ruleset::RuleSet::empty_for_test(),
            &effective,
            &launch,
            &assignments,
        )
        .unwrap();

        let alpha = sim.interner().get("Alpha").unwrap();
        let beta = sim.interner().get("Beta").unwrap();
        let neutral = sim.interner().get("Neutral").unwrap();
        let alpha_index = sim.houses().index_of(alpha).unwrap();
        let beta_index = sim.houses().index_of(beta).unwrap();
        let neutral_index = sim.houses().index_of(neutral).unwrap();
        assert_eq!(sim.session().current_house(), Some(alpha));
        assert!(sim.house_is_locally_controlled_player(alpha));
        assert!(!sim.house_is_locally_controlled_player(beta));
        assert!(sim.house_is_locally_controlled_player(neutral));
        assert!(!sim.house_is_human_player(neutral));
        assert!(sim.houses()[&neutral].is_human());
        assert!(sim.session().network_source_controls_house(0, alpha_index));
        assert!(!sim.session().network_source_controls_house(0, beta_index));
        assert!(
            sim.session()
                .network_source_controls_house(0, neutral_index)
        );
        assert!(!sim.session().network_source_controls_house(1, alpha_index));
        assert!(sim.session().network_source_controls_house(1, beta_index));
        assert!(
            sim.session()
                .network_source_controls_house(1, neutral_index)
        );

        let bytes = crate::sim::snapshot::GameSnapshot::save(&sim, 0, 0, "coop", 0);
        let mut restored = crate::sim::snapshot::GameSnapshot::load(&bytes)
            .unwrap()
            .sim;
        restored.restore_after_snapshot_load().unwrap();
        assert!(
            restored
                .session()
                .network_source_controls_house(0, neutral_index)
        );
        assert!(
            !restored
                .session()
                .network_source_controls_house(0, beta_index)
        );

        let mut guest_launch = launch.clone();
        guest_launch.local_player_index = 1;
        let mut guest_sim = crate::sim::world::Simulation::new();
        guest_sim
            .session_mut_for_test()
            .set_game_mode(ScenarioGameMode::Lan);
        guest_sim.session_mut_for_test().install_network_timing(5);
        crate::sim::scenario_bootstrap::initialize_vera_cooperative_houses(
            &mut guest_sim,
            &roster,
            &crate::rules::ruleset::RuleSet::empty_for_test(),
            &effective,
            &guest_launch,
            &assignments,
        )
        .unwrap();
        assert_eq!(guest_sim.session().current_house(), Some(beta));
        assert_eq!(sim.state_hash(), guest_sim.state_hash());
    }

    fn fixture() -> SkirmishLaunchSession {
        let profile = crate::test_fixture::json("tools/map_observation.command-bar.example.json");
        serde_json::from_value(profile["launch"].clone()).unwrap()
    }

    #[test]
    fn one_human_can_start_against_an_actual_ai_slot() {
        let mut launch = fixture();
        assert_eq!(launch.humans.len(), 1);
        assert_eq!(launch.opponents.len(), 1);
        assert!(has_minimum_players(&launch));

        launch.opponents.clear();
        assert_eq!(launch.ai_player_count, 1);
        assert!(
            !has_minimum_players(&launch),
            "a declared count without an AI House is not an opponent"
        );
    }

    #[test]
    fn observers_do_not_replace_playing_humans_or_opponents() {
        let mut launch = fixture();
        launch.humans[0].set_observer(true);
        assert!(!has_minimum_players(&launch));

        let mut player = fixture().local_player().clone();
        player.name = "Guest".into();
        player.raw_slot_index = Some(2);
        launch.humans.push(player);
        launch.opponents.clear();
        assert!(!has_minimum_players(&launch));

        // VERA shared control: two players on one start are one House.
        launch.humans[0].set_observer(false);
        assert!(!has_minimum_players(&launch));
    }
}
