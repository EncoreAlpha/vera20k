//! VMCE setup state and its map-authored role metadata projection.

use super::{AppState, NetworkSetupProtocol};

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
}
