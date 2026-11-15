//! VMCE setup state, its map-authored role metadata projection, and its
//! lobby role-choice/selection/admission decisions.

use super::{AppState, NetworkSetupProtocol};
use crate::skirmish_launch::{SkirmishHumanSlot, SkirmishLaunchSession};

/// This build owns the complete VMCE setup parser and series admission path.
/// Peer compatibility is validated by the existing options transaction below.
pub(super) fn ensure_host_setup_supported(
    _state: &mut AppState,
    _launch: &SkirmishLaunchSession,
) -> anyhow::Result<()> {
    Ok(())
}

pub(crate) fn supports_host_setup(_mode: crate::skirmish_modes::MpModeClass) -> bool {
    true
}

/// Add the VMCE-specific map/config projection to the shared stock-map result.
/// The service computes the generic catalog/admission result once, then
/// commits this finalized projection through MatchSession.
pub(super) fn apply_vmce_host_setup_map(
    state: &AppState,
    host: bool,
    wol: bool,
    launch: &crate::skirmish_launch::SkirmishLaunchSession,
    metadata: &crate::app::match_runtime::session::NetworkMapMetadata,
    map_changed: bool,
    vmce_config: Option<&crate::app::match_bootstrap::NetworkVmceMapConfig>,
    mut outcome: super::HostSetupMapOutcome,
) -> anyhow::Result<super::HostSetupMapOutcome> {
    use crate::app::match_runtime::session::NetworkMapAdmission;

    let vmce = launch.mode.class.is_vmce();
    outcome.vmce_config_changed =
        state.match_state.session.network_vmce_map_config() != vmce_config;
    validate_host_vmce_map_config(
        host,
        wol,
        vmce,
        launch.selected_map_file.as_deref(),
        &metadata.source_sha256,
        vmce_config,
    )?;
    if !vmce {
        return Ok(outcome);
    }

    let (vmce_local_map, vmce_local_source) = if let Some(config) = vmce_config
        && let Some(file) = launch.selected_map_file.as_deref()
        && let (Some(game_config), Some(assets)) = (
            state.platform.game_config.as_ref(),
            state.process_assets.manager(),
        )
        && let Ok(map) =
            crate::map::source::load_network_map_ini(&game_config.paths.ra2_dir, file, assets)
        && map
            .source
            .physical_identity()
            .is_some_and(|(_, digest)| digest == config.source_sha256)
    {
        // The peer's local map validates its locally derived campaign
        // projection before it enters Session. Coop LAN never transfers
        // a missing or mismatched map/config.
        config.apply_to_map(file, &config.source_sha256, &map.ini)?;
        let loose = matches!(
            &map.source,
            crate::map::source::LoadedMapSource::Loose { .. }
        );
        // Campaign entries are not guest Choose Map rows. Their locally
        // verified bytes provide the retained source used after GO.
        let name = map
            .ini
            .section("Basic")
            .map(|section| section.read_string("Name", "", 0x20))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| metadata.display_name.clone());
        (
            Some((name, loose)),
            Some((map.source, metadata.effective_digest.clone())),
        )
    } else {
        (None, None)
    };

    outcome.local_map = outcome.local_map.take().or(vmce_local_map);
    // VMCE LAN has no transfer pass, so the guest reanswers every host-options
    // revision even when the selected map identity is unchanged.
    let reanswer_map = should_reanswer_map(vmce, host, wol, map_changed);
    outcome.map_admission = if !wol && !host && vmce_config.is_none() {
        Some(NetworkMapAdmission::Unavailable)
    } else if reanswer_map {
        let file = launch
            .selected_map_file
            .as_deref()
            .filter(|file| !file.is_empty())
            .ok_or_else(|| anyhow::anyhow!("host options have no map"))?;
        Some(metadata.admission(file, outcome.local_map.is_some()))
    } else {
        None
    };
    outcome.request_preview = false;
    outcome.vmce_local_source = vmce_local_source;
    outcome.vmce_series_admission = !host && !wol;
    Ok(outcome)
}

fn validate_host_vmce_map_config(
    host: bool,
    wol: bool,
    vmce: bool,
    map_file: Option<&str>,
    source_sha256: &str,
    config: Option<&crate::app::match_bootstrap::NetworkVmceMapConfig>,
) -> anyhow::Result<()> {
    match (vmce, config) {
        (true, Some(config)) => {
            anyhow::ensure!(
                !wol,
                "WOL VMCE does not synchronize supplemental map config"
            );
            let file = map_file
                .ok_or_else(|| anyhow::anyhow!("VMCE host options have no selected map"))?;
            config.validate_binding(file, source_sha256)?;
        }
        (true, None) if wol || !host => {}
        (true, None) => anyhow::bail!("VMCE host options omit the map config"),
        (false, Some(_)) => anyhow::bail!("non-VMCE host options carry VMCE map config"),
        (false, None) => {}
    }
    Ok(())
}

fn should_reanswer_map(vmce: bool, host: bool, wol: bool, map_changed: bool) -> bool {
    map_changed || (vmce && !host && !wol)
}

/// VMCE's lobby start permission. The shared start command keeps the native
/// ordering, readiness and publishing; this owner decides whether the selected
/// authored map, its synchronized config, the accepted shared seed and the
/// room's role claims allow the start to proceed to GO, and owns the
/// player-facing reason when they do not.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VmceStartPermission {
    Allowed,
    Denied(&'static str),
}

pub(super) fn start_permission(
    state: &AppState,
    launch: &SkirmishLaunchSession,
) -> VmceStartPermission {
    // A recognized mode id is not a start permit. The validated catalog record
    // is the runtime availability owner.
    if !super::vmce_map::mode_available(state) {
        return VmceStartPermission::Denied(
            "VMCE cannot start because this build has no playable VMCE backend for the selected campaign.",
        );
    }
    let Some(file) = launch.selected_map_file.as_deref() else {
        return VmceStartPermission::Denied("VMCE requires a selected map.");
    };
    if state.match_state.session_mode() != crate::app::match_runtime::session::SessionMode::Lan {
        return VmceStartPermission::Denied("VMCE map configuration sync is currently LAN-only.");
    }
    let Some(config) = state.match_state.session.network_vmce_map_config().cloned() else {
        return VmceStartPermission::Denied(
            "VMCE host options omit the selected map configuration.",
        );
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
    match map {
        Ok(map) => match state
            .match_state
            .session
            .network_start_seed()
            .ok_or_else(|| anyhow::anyhow!("VMCE start has no accepted shared seed"))
            .and_then(|seed| {
                let (_, source_sha256) = map
                    .source
                    .physical_identity()
                    .ok_or_else(|| anyhow::anyhow!("VMCE map has no physical identity"))?;
                crate::app::match_bootstrap::resolve_vmce_houses_with_config(
                    &map.ini,
                    file,
                    source_sha256,
                    &config,
                    launch,
                    seed,
                )
            }) {
            Ok(assignments) => {
                // The map, role and House configuration is valid, so the start
                // proceeds to GO. The accepted start freezes these assignments
                // and the loader installs the shared House command grants from
                // the same effective map.
                log::info!(
                    "VMCE role roster resolves to {} player Houses",
                    assignments.participants.len()
                );
                VmceStartPermission::Allowed
            }
            Err(error) => {
                log::warn!("VMCE map role configuration is invalid: {error}");
                VmceStartPermission::Denied(
                    "VMCE cannot start: there may be more players than roles, or selected roles may be missing or duplicated.",
                )
            }
        },
        Err(error) => {
            log::warn!("VMCE could not read map role configuration: {error:#}");
            VmceStartPermission::Denied("VMCE could not read role data from the selected map.")
        }
    }
}

/// VMCE's authored campaign supplies its opponents on the map. Its room needs
/// a player, but must not admit an observer as a campaign role.
pub(super) fn minimum_players_met(launch: &SkirmishLaunchSession) -> bool {
    !launch.humans.is_empty() && launch.humans.iter().all(|human| !human.is_observer())
}

/// The VMCE assignments an accepted start freezes.
///
/// This is the one place a VMCE start reads the shared lobby difficulty row:
/// the frozen assignments carry it into the loader's TriggerType flags and the
/// result page, and every later consumer reads the frozen copy rather than the
/// process row a later room may have changed.
pub(super) fn frozen_start_houses(
    state: &AppState,
    launch: &SkirmishLaunchSession,
    agreed_seed: u32,
) -> anyhow::Result<Option<crate::skirmish_launch::VmceHouseAssignments>> {
    if !launch.mode.class.is_vmce() {
        return Ok(None);
    }
    let file = launch
        .selected_map_file
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("VMCE start has no selected map"))?;
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
    let map = crate::map::source::load_network_map_ini(root, file, assets)?;
    let config = state
        .match_state
        .session
        .network_vmce_map_config()
        .ok_or_else(|| anyhow::anyhow!("VMCE start has no accepted map config"))?;
    let (_, source_sha256) = map
        .source
        .physical_identity()
        .ok_or_else(|| anyhow::anyhow!("VMCE map has no physical identity"))?;
    let mut assignments = crate::app::match_bootstrap::resolve_vmce_houses_with_config(
        &map.ini,
        file,
        source_sha256,
        config,
        launch,
        agreed_seed,
    )?;
    // The lane-synchronized lobby row is this mission's TriggerType difficulty
    // (0=Easy, 1=Normal, 2=Hard). Every peer receives the same row through the
    // LAN options payload, so the frozen assignment identity stays identical.
    assignments.difficulty = state
        .frontend
        .multiplayer_shell_runtime
        .lobby_difficulty()
        .native_row();
    Ok(Some(assignments))
}

/// One selectable authored player role, or the Random sentinel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VmceRoleChoice {
    pub value: i32,
    /// The authored House owner; `None` for the Random row.
    pub owner: Option<String>,
}

/// A row's VMCE role choice resolved to the shared launch/session indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum VmceRoleSelection {
    Host {
        index: usize,
        role: crate::skirmish_launch::LaunchHumanRole,
    },
    Guest {
        index: usize,
        row: u8,
        slot: crate::skirmish_launch::SkirmishLocalSlot,
        role: crate::skirmish_launch::LaunchHumanRole,
    },
}

/// VMCE's lobby role choice owner: the row's current claim and the authored
/// roles it may still take. A shared view renders these; it does not decide
/// which claims are available.
pub(super) fn role_choices(
    launch: &SkirmishLaunchSession,
    row: u8,
    details: &VmceMapDetails,
) -> (i32, Vec<VmceRoleChoice>) {
    let assigned = launch
        .humans
        .iter()
        .filter(|human| human.raw_slot_index.is_some() && human.raw_slot_index != Some(row))
        .map(|human| human.role.native_value())
        .collect::<Vec<_>>();
    let current = launch
        .humans
        .iter()
        .find(|human| human.raw_slot_index == Some(row))
        .map_or(-2, |human| human.role.native_value());
    let mut choices = vec![VmceRoleChoice {
        value: -2,
        owner: None,
    }];
    for (index, owner) in details.owners.iter().enumerate() {
        let Some(owner) = owner.as_deref() else {
            continue;
        };
        let value = index as i32;
        if assigned.contains(&value) && current != value {
            continue;
        }
        choices.push(VmceRoleChoice {
            value,
            owner: Some(owner.to_owned()),
        });
    }
    (current, choices)
}

/// VMCE's own role-claim rule: a guest may assign only its own row, while the
/// host may assign any. The shared action still mutates the authoritative
/// launch and publishes the options/PlayerChoices.
pub(super) fn role_selection(
    launch: &SkirmishLaunchSession,
    row: u8,
    value: i32,
    host: bool,
) -> anyhow::Result<VmceRoleSelection> {
    anyhow::ensure!(launch.mode.class.is_vmce(), "role selection outside VMCE");
    let index = launch
        .humans
        .iter()
        .position(|human| human.raw_slot_index == Some(row))
        .ok_or_else(|| anyhow::anyhow!("role row has no player"))?;
    anyhow::ensure!(
        host || index == launch.local_player_index,
        "guests may only assign their own role"
    );
    let role = crate::skirmish_launch::LaunchHumanRole::from_native(value);
    if host {
        return Ok(VmceRoleSelection::Host { index, role });
    }
    Ok(VmceRoleSelection::Guest {
        index,
        row,
        slot: launch.local_player().slot.clone(),
        role,
    })
}

/// Validate an incoming VMCE role change before the shared peer-choice
/// transaction mutates the roster.
pub(super) fn validate_peer_role(
    state: &AppState,
    index: usize,
    role: crate::skirmish_launch::LaunchHumanRole,
) -> anyhow::Result<()> {
    state
        .match_state
        .session
        .validate_network_vmce_role(index, role)
}

/// Apply an incoming VMCE role change after the shared peer-choice transaction
/// has accepted the sender's other fields. The session remains the
/// authoritative role-state owner.
pub(super) fn accept_peer_role(
    state: &mut AppState,
    index: usize,
    role: crate::skirmish_launch::LaunchHumanRole,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        state
            .match_state
            .session
            .network_setup()
            .is_some_and(|launch| launch.mode.class.is_vmce()),
        "role update outside VMCE"
    );
    state.match_state.session.set_network_vmce_role(index, role)
}

/// A map or mode change clears every VMCE claim to Random before the shared
/// map transaction publishes the new role rows.
pub(super) fn reset_roles_after_map_change(state: &mut AppState) -> anyhow::Result<()> {
    let Some(launch) = state.match_state.session.network_setup() else {
        return Ok(());
    };
    if !launch.mode.class.is_vmce() {
        return Ok(());
    }
    let count = launch.humans.len();
    for index in 0..count {
        state.match_state.session.set_network_vmce_role(
            index,
            crate::skirmish_launch::LaunchHumanRole::Player { group: -2 },
        )?;
    }
    Ok(())
}

// ---- VMCE's lobby mode events -------------------------------------------
//
// The shared setup owns the room, host, roster, admission, network and
// reliable queue; these are the VMCE mode object's lobby slots, reached only
// through `super::dispatch` when the event's mode is VMCE. Native's join
// (`0x005E2FA0`), departure (`0x005E11DC`), setup-open (BC
// `0x005ED6FE..0x005ED717`), parse (`0x005E541D`) and Use Map (`0x005E7160`)
// slots for an ordinary mode
// do nothing; VMCE keeps its campaign-role rules here so a Cooperative lobby
// change cannot reach it.

/// Whether a mode class is VMCE. The shared dispatch asks this so a VMCE lobby
/// event never passes through the Cooperative owner.
pub(super) fn handles_mode(mode: Option<crate::skirmish_modes::MpModeClass>) -> bool {
    mode.is_some_and(|mode| mode.is_vmce())
}

/// The host's join accept: a playing seat starts on Random, because the join
/// packet's `Player1` default is not a role choice. Observers keep their role.
pub(super) fn admit_join_slot(human: &mut SkirmishHumanSlot) {
    if !human.is_observer() {
        human.role = crate::skirmish_launch::LaunchHumanRole::Player { group: -2 };
    }
}

/// BC open opens the room; VMCE's series and map state are committed by the
/// host's options transaction (`apply_vmce_host_setup_map`) and Use Map.
pub(super) fn host_opened(_state: &mut AppState) -> anyhow::Result<()> {
    Ok(())
}

/// The shared join admission rewrites the roster and the host republishes the
/// options; the VMCE role slots travel inside those options.
pub(super) fn guest_joined(_state: &mut AppState) -> anyhow::Result<()> {
    Ok(())
}

/// The shared removal and republish own the roster; the remaining VMCE seats
/// keep their claims.
pub(super) fn guest_left(_state: &mut AppState) -> anyhow::Result<()> {
    Ok(())
}

/// A changed mode's entry accepts the VMCE roles in the shared host-setup
/// transaction (`0x5E542B..` in `service`), not in this slot.
pub(super) fn mode_entered(_state: &mut AppState) -> anyhow::Result<()> {
    Ok(())
}

/// Every host-options parse refreshes Cooperative's campaign row; VMCE's roles
/// are accepted with the options themselves.
pub(super) fn parse_refresh(_state: &mut AppState) {}

/// Use Map's VMCE series/config commit is part of the shared map transaction
/// (`commit_map_selection`); the mode slot adds nothing.
pub(super) fn map_used(
    _state: &mut AppState,
    _mode_changed: bool,
    _row: usize,
) -> anyhow::Result<()> {
    Ok(())
}

/// Parsed map-authored Player1..Player4 labels and their House presentation
/// metadata. The lobby reads this through [`VmceLobbyProjection`].
#[derive(Debug, Clone, Default)]
pub(crate) struct VmceMapDetails {
    pub owners: [Option<String>; 4],
    /// Each role House's authored `Country=`, for the lobby's role board.
    pub countries: [Option<String>; 4],
    pub colors: [Option<[u8; 3]>; 4],
    pub briefing: Option<String>,
}

impl VmceMapDetails {
    /// The map's actual selectable primary roles: one authored `PlayerN` House
    /// per row and slot. Without a resolved map the four-row shape stays.
    pub(crate) fn role_count(&self) -> usize {
        let count = self.owners.iter().filter(|owner| owner.is_some()).count();
        if count == 0 { 4 } else { count.min(4) }
    }
}

/// A VMCE guest's local campaign/map problem, shown in the lobby's inline
/// status bar after the host's options revision is parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmceLobbyStatus {
    /// The host's map or its campaign.ini is not installed locally.
    MissingAssets,
    /// A local copy exists but its bytes or series differ from the host's.
    MapMismatch,
}

/// Read-only presentation inputs owned by the network setup protocol.
pub(crate) struct VmceLobbyProjection {
    pub(crate) details: VmceMapDetails,
    pub(crate) status: Option<VmceLobbyStatus>,
    pub(crate) retry_allowed: bool,
}

#[derive(Default)]
pub(super) struct VmceSetupState {
    /// Parsed Player1..Player4 metadata keyed by the selected map identity.
    role_cache: std::cell::RefCell<Option<(std::path::PathBuf, String, String, VmceMapDetails)>>,
    lobby_status: Option<VmceLobbyStatus>,
    retry_allowed: bool,
}

impl VmceSetupState {
    fn cached_role_details(
        &self,
        root: &std::path::Path,
        file: &str,
        digest: &str,
    ) -> Option<VmceMapDetails> {
        self.role_cache
            .borrow()
            .as_ref()
            .filter(|(cached_root, cached_file, cached_digest, _)| {
                cached_root == root && cached_file == file && cached_digest == digest
            })
            .map(|(_, _, _, details)| details.clone())
    }

    fn cache_role_details(
        &self,
        root: std::path::PathBuf,
        file: String,
        digest: String,
        details: VmceMapDetails,
    ) {
        self.role_cache.replace(Some((root, file, digest, details)));
    }

    fn clear_role_cache(&self) {
        self.role_cache.replace(None);
    }

    fn accept_options(&mut self, status: Option<VmceLobbyStatus>) {
        self.clear_role_cache();
        self.lobby_status = status;
        self.retry_allowed = false;
    }

    fn projection(&self, details: VmceMapDetails) -> VmceLobbyProjection {
        VmceLobbyProjection {
            details,
            status: self.lobby_status,
            retry_allowed: self.retry_allowed,
        }
    }
}

impl NetworkSetupProtocol {
    /// Replace the prior UI projection only after the matching Options
    /// transaction commits. The LAN admission owner may then reapply its
    /// current failed verdict as the new retry affordance.
    pub(crate) fn reset_vmce_lobby_options(&mut self, status: Option<VmceLobbyStatus>) {
        self.vmce_setup.accept_options(status);
    }

    /// Record or clear a VMCE guest's local campaign/map admission problem.
    /// Display state only; the map-status owner remains the admission flag.
    pub(crate) fn set_vmce_lobby_status(&mut self, status: Option<VmceLobbyStatus>) {
        self.vmce_setup.lobby_status = status;
    }

    /// Permit a guest to retry Ready after a VMCE boundary scan rejected the
    /// current map. This only affects the setup UI's Accept affordance.
    pub(crate) fn allow_vmce_retry(&mut self) {
        self.vmce_setup.retry_allowed = true;
    }

    /// Clear the temporary retry affordance after the retry is consumed.
    pub(crate) fn clear_vmce_retry(&mut self) {
        self.vmce_setup.retry_allowed = false;
    }

    /// Shared read-only data for VMCE lobby rendering, refreshed against the
    /// current local map/options identity before consumers use its metadata.
    pub(crate) fn vmce_lobby_projection(&self, state: &AppState) -> VmceLobbyProjection {
        let details = map_details(state);
        self.vmce_setup.projection(details)
    }

    pub(super) fn cached_vmce_role_details(
        &self,
        root: &std::path::Path,
        file: &str,
        digest: &str,
    ) -> Option<VmceMapDetails> {
        self.vmce_setup.cached_role_details(root, file, digest)
    }

    pub(super) fn cache_vmce_role_details(
        &self,
        root: std::path::PathBuf,
        file: String,
        digest: String,
        details: VmceMapDetails,
    ) {
        self.vmce_setup
            .cache_role_details(root, file, digest, details);
    }

    pub(super) fn clear_vmce_role_cache(&self) {
        self.vmce_setup.clear_role_cache();
    }
}

/// Load the local map once per exact settings root, selected file and frozen
/// source/config digest. This used to live in the per-frame shared view.
pub(super) fn map_details(state: &AppState) -> VmceMapDetails {
    let session = &state.match_state.session;
    let vmce_config = session.network_vmce_map_config();
    // During a result round the editable launch already names the next map;
    // its retained config still belongs to the mission that just finished.
    let Some(file) = vmce_config
        .map(|config| config.map_file.as_str())
        .or_else(|| session.network_setup()?.selected_map_file.as_deref())
    else {
        if let Some(protocol) = state.frontend.network_setup_protocol.as_ref() {
            protocol.clear_vmce_role_cache();
        }
        return VmceMapDetails::default();
    };
    let (Some(config), Some(assets)) = (
        state.platform.game_config.as_ref(),
        state.process_assets.manager(),
    ) else {
        if let Some(protocol) = state.frontend.network_setup_protocol.as_ref() {
            protocol.clear_vmce_role_cache();
        }
        return VmceMapDetails::default();
    };
    let root = config.paths.settings_dir().join("Maps");
    let source_digest = state
        .match_state
        .session
        .network_map_metadata()
        .map_or_else(String::new, |metadata| metadata.source_sha256.clone());
    let digest = format!(
        "{}:{}",
        source_digest,
        vmce_config.map_or("", |config| config.config_sha256.as_str())
    );
    let key_file = file.to_ascii_lowercase();
    let protocol = state.frontend.network_setup_protocol.as_ref();
    if let Some(details) =
        protocol.and_then(|protocol| protocol.cached_vmce_role_details(&root, &key_file, &digest))
    {
        return details;
    }

    let details = crate::map::source::load_network_map_ini(&root, file, assets)
        .ok()
        .and_then(|loaded| {
            let actual_digest = loaded
                .source
                .physical_identity()
                .map_or("", |(_, digest)| digest);
            let mut details = role_metadata_with_config(
                &loaded.ini,
                state.rules(),
                file,
                actual_digest,
                vmce_config,
            )?;
            // The campaign's Brief= is a movie; mission text lives in
            // MISSIONMD.INI under the scenario filename (restate reader).
            let briefing = crate::rules::retail_sources::select_ini(assets, "MISSIONMD.INI")
                .ok()
                .and_then(|mission| {
                    let section = mission.ini.section(file)?;
                    if !section.is_present("Briefing") {
                        return None;
                    }
                    let key =
                        crate::rules::campaign_loading::restate_briefing_key(&mission.ini, file);
                    state
                        .process_assets
                        .csf
                        .as_ref()?
                        .get(&key)
                        .map(str::to_owned)
                });
            details.briefing = briefing;
            Some(details)
        })
        .unwrap_or_default();
    if let Some(protocol) = protocol {
        protocol.cache_vmce_role_details(root, key_file, digest, details.clone());
    }
    details
}

pub(super) fn role_metadata(
    ini: &crate::rules::ini_parser::IniFile,
    rules: Option<&crate::rules::ruleset::RuleSet>,
) -> VmceMapDetails {
    let basic = crate::map::basic::parse_basic_section(ini);
    let schemes = rules.map_or(&[][..], |rules| rules.color_schemes.as_slice());
    let roster = crate::map::houses::parse_house_roster(ini, schemes, rules);
    if basic
        .campaign_players
        .validate_against_roster(&roster)
        .is_err()
    {
        return VmceMapDetails::default();
    }
    let owners = basic.campaign_players.players.map(|player| player.owner);
    let houses: [Option<&crate::map::houses::HouseDefinition>; 4] = std::array::from_fn(|i| {
        roster
            .houses
            .iter()
            .find(|house| Some(house.name.as_str()) == owners[i].as_deref())
    });
    let colors = std::array::from_fn(|i| {
        let house = houses[i]?;
        let scheme = schemes.get(usize::from(house.color.0))?;
        Some(crate::rules::color_scheme::hsv_to_rgb(scheme.hsv))
    });
    let countries =
        std::array::from_fn(|i| houses[i].as_ref().and_then(|house| house.country.clone()));
    VmceMapDetails {
        owners,
        countries,
        colors,
        briefing: None,
    }
}

pub(super) fn role_metadata_with_config(
    ini: &crate::rules::ini_parser::IniFile,
    rules: Option<&crate::rules::ruleset::RuleSet>,
    file: &str,
    source_sha256: &str,
    config: Option<&crate::app::match_bootstrap::NetworkVmceMapConfig>,
) -> Option<VmceMapDetails> {
    match config {
        Some(config) => {
            let effective = config.apply_to_map(file, source_sha256, ini).ok()?;
            Some(role_metadata(&effective, rules))
        }
        None => Some(role_metadata(ini, rules)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_options_clear_old_vmce_error_retry_and_role_cache() {
        let mut protocol = NetworkSetupProtocol::new(
            crate::ui::multiplayer_setup::SetupDialog {
                host: false,
                flavor: crate::ui::multiplayer_setup::SetupFlavor::Lan,
                modern: true,
            },
            false,
        );
        protocol.vmce_setup.lobby_status = Some(VmceLobbyStatus::MapMismatch);
        protocol.vmce_setup.retry_allowed = true;
        protocol.vmce_setup.cache_role_details(
            std::path::PathBuf::from("Maps"),
            "old.map".to_owned(),
            "old-digest".to_owned(),
            VmceMapDetails::default(),
        );

        protocol.reset_vmce_lobby_options(Some(VmceLobbyStatus::MissingAssets));

        assert_eq!(
            protocol.vmce_setup.lobby_status,
            Some(VmceLobbyStatus::MissingAssets)
        );
        assert!(!protocol.vmce_setup.retry_allowed);
        assert!(protocol.vmce_setup.role_cache.borrow().is_none());
    }

    #[test]
    fn options_map_admission_reanswers_vmce_guests_and_only_changed_stock_maps() {
        assert!(should_reanswer_map(true, false, false, false));
        assert!(should_reanswer_map(true, false, false, true));
        assert!(!should_reanswer_map(true, true, false, false));
        assert!(!should_reanswer_map(true, false, true, false));
        assert!(!should_reanswer_map(false, false, false, false));
        assert!(should_reanswer_map(false, false, false, true));
    }

    #[test]
    fn a_joining_playing_seat_defaults_to_random_and_spectators_keep_their_role() {
        use crate::skirmish_launch::{
            LaunchCountry, LaunchHumanRole, LaunchStartPosition, LaunchTeam,
        };

        let join_human = |role| crate::skirmish_launch::SkirmishHumanSlot {
            raw_slot_index: Some(1),
            name: "Guest".into(),
            slot: crate::skirmish_launch::SkirmishLocalSlot {
                country: LaunchCountry::America,
                country_random: false,
                color_index: 1,
                color_random: false,
                start_position: LaunchStartPosition::Auto,
                team: LaunchTeam::None,
            },
            role,
        };
        // The join packet's Player1 default is not a role choice.
        let mut player = join_human(LaunchHumanRole::Player { group: 1 });
        admit_join_slot(&mut player);
        assert_eq!(player.role, LaunchHumanRole::Player { group: -2 });
        // An observer's role is not a playing claim.
        let mut watcher = join_human(LaunchHumanRole::Observer);
        admit_join_slot(&mut watcher);
        assert!(watcher.is_observer());
    }

    #[test]
    fn vmce_lan_local_map_admission_never_advertises_transfer() {
        use crate::app::match_runtime::session::NetworkMapAdmission::{
            NeedsTransfer, Present, Unavailable,
        };
        use crate::app::match_runtime::session::SessionMode::{Lan, Wol};
        use crate::skirmish_modes::MpModeClass::{Battle, Vmce};

        let policy =
            crate::app::lan_service::local_map_admission_policy(Lan, Some(Vmce), NeedsTransfer);
        assert_eq!(policy.admission, Unavailable);
        assert!(policy.report_missing_vmce_map);
        let policy = crate::app::lan_service::local_map_admission_policy(Lan, Some(Vmce), Present);
        assert_eq!(policy.admission, Present);
        assert!(!policy.report_missing_vmce_map);
        let policy =
            crate::app::lan_service::local_map_admission_policy(Lan, Some(Battle), NeedsTransfer);
        assert_eq!(policy.admission, NeedsTransfer);
        assert!(!policy.report_missing_vmce_map);
        let policy =
            crate::app::lan_service::local_map_admission_policy(Wol, Some(Vmce), NeedsTransfer);
        assert_eq!(policy.admission, NeedsTransfer);
        assert!(!policy.report_missing_vmce_map);
    }

    #[test]
    fn host_vmce_map_config_validation_keeps_wol_and_missing_guest_rules() {
        let map_file = "test.map";
        let source = "a".repeat(64);
        let config = crate::app::match_bootstrap::NetworkVmceMapConfig::new(
            map_file.to_owned(),
            source.clone(),
            crate::map::vmce_config::VmceMapConfig::default(),
        )
        .unwrap();

        assert!(
            validate_host_vmce_map_config(true, true, true, Some(map_file), &source, None).is_ok()
        );
        assert!(validate_host_vmce_map_config(
            true,
            true,
            true,
            Some(map_file),
            &source,
            Some(&config),
        )
        .is_err());
        assert!(
            validate_host_vmce_map_config(false, false, true, Some(map_file), &source, None)
                .is_ok()
        );
        assert!(
            validate_host_vmce_map_config(true, false, true, Some(map_file), &source, None)
                .is_err()
        );
        assert!(
            validate_host_vmce_map_config(false, false, false, Some(map_file), &source, None)
                .is_ok()
        );
    }

    fn role_launch() -> SkirmishLaunchSession {
        use crate::skirmish_launch::LaunchHumanRole;
        let profile = crate::test_fixture::json("tools/map_observation.command-bar.example.json");
        let mut launch: SkirmishLaunchSession =
            serde_json::from_value(profile["launch"].clone()).unwrap();
        launch.mode.class = crate::skirmish_modes::MpModeClass::Vmce;
        let mut guest = launch.humans[0].clone();
        guest.raw_slot_index = Some(1);
        guest.name = "Guest".to_owned();
        guest.role = LaunchHumanRole::Player { group: -2 };
        launch.humans.push(guest);
        launch.local_player_index = 1;
        launch
    }

    #[test]
    fn vmce_role_choices_offer_unclaimed_authors_and_keep_the_current_claim() {
        use crate::skirmish_launch::LaunchHumanRole;
        let mut launch = role_launch();
        launch.humans[0].role = LaunchHumanRole::Player { group: 0 };
        launch.humans[1].role = LaunchHumanRole::Player { group: -2 };
        let details = VmceMapDetails {
            owners: [
                Some("Alpha".to_owned()),
                Some("Beta".to_owned()),
                None,
                None,
            ],
            ..VmceMapDetails::default()
        };

        let (current, choices) = role_choices(&launch, 1, &details);
        assert_eq!(current, -2);
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.value)
                .collect::<Vec<_>>(),
            vec![-2, 1]
        );
        assert_eq!(choices[0].owner, None);
        assert_eq!(choices[1].owner.as_deref(), Some("Beta"));

        let (current, choices) = role_choices(&launch, 0, &details);
        assert_eq!(current, 0);
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.value)
                .collect::<Vec<_>>(),
            vec![-2, 0, 1],
            "the current claim stays listed and an unclaimed author is added"
        );
        assert_eq!(choices[1].owner.as_deref(), Some("Alpha"));
        assert_eq!(choices[2].owner.as_deref(), Some("Beta"));

        launch.humans[1].role = LaunchHumanRole::Player { group: 1 };
        let (_, choices) = role_choices(&launch, 0, &details);
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.value)
                .collect::<Vec<_>>(),
            vec![-2, 0],
            "another row's explicit claim is not offered"
        );
    }

    #[test]
    fn vmce_role_selection_restricts_a_guest_to_its_own_row() {
        use crate::skirmish_launch::LaunchHumanRole;
        let launch = role_launch();

        assert!(role_selection(&launch, 0, 2, false).is_err());
        let selection = role_selection(&launch, 1, 2, false).unwrap();
        match selection {
            VmceRoleSelection::Guest {
                index,
                row,
                slot,
                role,
            } => {
                assert_eq!(index, 1);
                assert_eq!(row, 1);
                assert_eq!(slot, launch.local_player().slot);
                assert_eq!(role, LaunchHumanRole::Player { group: 2 });
            }
            VmceRoleSelection::Host { .. } => panic!("guest selection became a host action"),
        }

        let selection = role_selection(&launch, 0, 2, true).unwrap();
        match selection {
            VmceRoleSelection::Host { index, role } => {
                assert_eq!(index, 0);
                assert_eq!(role, LaunchHumanRole::Player { group: 2 });
            }
            VmceRoleSelection::Guest { .. } => panic!("host selection became a guest action"),
        }
    }

    #[test]
    fn the_vmce_lobby_requires_a_player_and_never_counts_an_observer_as_one() {
        let profile = crate::test_fixture::json("tools/map_observation.command-bar.example.json");
        let launch: SkirmishLaunchSession =
            serde_json::from_value(profile["launch"].clone()).unwrap();
        assert!(minimum_players_met(&launch));
        let mut observer = launch.clone();
        observer.humans[0].set_observer(true);
        assert!(!minimum_players_met(&observer));
        let mut empty = launch.clone();
        empty.humans.clear();
        assert!(!minimum_players_met(&empty));
    }
}
