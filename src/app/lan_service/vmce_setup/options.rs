//! VMCE-specific LAN Options identity, validation, and admission semantics.

use anyhow::{Result, anyhow, ensure};

use crate::app::AppState;
use crate::app::match_runtime::session::MatchSession;
use crate::app::shell_network_setup::VmceLobbyStatus;
use crate::skirmish_launch::{LaunchHumanRole, SkirmishLaunchSession};

use super::SeriesIdentity;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::app::lan_service) struct VmceSeriesIdentity {
    pub(in crate::app::lan_service) directory: String,
    pub(in crate::app::lan_service) sha256: String,
}

/// One VMCE role assignment keyed by the synchronized lobby row (the native
/// `raw_slot_index` seat), never by the publisher's local participant order:
/// LAN admits participants local-first, so the same people can be ordered
/// differently on the two peers.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::app::lan_service) struct VmceRoleRow {
    pub(in crate::app::lan_service) row: u8,
    pub(in crate::app::lan_service) role: LaunchHumanRole,
}

/// The whole-series identity a VMCE LAN host advertises: its directory, the
/// `campaign.ini` bytes' SHA-256, and one content digest over the entire
/// series. `Ok(None)` while the content scan still runs; the caller defers the
/// revision rather than blocking the LAN service loop.
pub(in crate::app::lan_service) fn published_series_identity(
    state: &mut AppState,
) -> Result<Option<(VmceSeriesIdentity, String)>> {
    let session = &state.match_state.session;
    if !session
        .network_setup()
        .is_some_and(|launch| launch.mode.class.is_vmce())
    {
        return Ok(None);
    }
    let config = session
        .network_vmce_map_config()
        .ok_or_else(|| anyhow!("VMCE host has no selected map config"))?;
    let file = session
        .network_setup()
        .and_then(|launch| launch.selected_map_file.as_deref())
        .ok_or_else(|| anyhow!("VMCE has no selected map"))?
        .to_owned();
    let source_sha256 = session
        .network_map_metadata()
        .ok_or_else(|| anyhow!("VMCE map metadata missing"))?
        .source_sha256
        .clone();
    ensure!(
        config.source_sha256 == source_sha256,
        "VMCE config is not bound to the selected map"
    );
    let config_series_directory = config.series_directory.clone();
    let config_series_sha256 = config.series_sha256.clone();

    let source = super::super::vmce_series::read_local_series(state, &file)?;
    let content = {
        let service = state
            .match_state
            .lan_service
            .as_mut()
            .ok_or_else(|| anyhow!("no LAN service"))?;
        service.vmce_setup.host_digest(&source)
    };
    let Some(content) = content else {
        return match super::super::vmce_series_state(state) {
            super::VmceSeriesState::Failed(error) => {
                Err(anyhow!("VMCE series content is invalid: {error}"))
            }
            _ => Ok(None),
        };
    };
    let (directory, sha256) =
        crate::app::vmce_campaign::host_vmce_identity(state, &file, &source_sha256)?;
    if !directory.eq_ignore_ascii_case(&source.directory) || sha256 != source.manifest_sha256() {
        // The series changed while its digest was computed: rescan the new
        // identity and defer this revision.
        let source = super::super::vmce_series::read_local_series(state, &file)?;
        if let Some(service) = state.match_state.lan_service.as_mut() {
            let _ = service.vmce_setup.host_digest(&source);
        }
        return Ok(None);
    }
    ensure!(
        config_series_directory.as_deref() == Some(directory.as_str())
            && config_series_sha256.as_deref() == Some(sha256.as_str()),
        "host VMCE series changed after map selection"
    );
    Ok(Some((VmceSeriesIdentity { directory, sha256 }, content)))
}

/// Guest identity from the active options scope. A snapshot from an earlier
/// setup scope is never used as current series evidence.
pub(in crate::app::lan_service) fn current_series_identity(
    state: &AppState,
) -> Option<SeriesIdentity> {
    let session = &state.match_state.session;
    let launch = session.network_setup()?;
    if !launch.mode.class.is_vmce() || session.host() {
        return None;
    }
    let service = state.match_state.lan_service.as_ref()?;
    let received = service
        .received_options
        .as_ref()
        .filter(|received| service.setup_scope() == Some(received.scope()))?;
    received.vmce_series_identity()
}

/// Validate an incoming VMCE identity and role projection before HostSetup
/// changes any session or admission state.
pub(in crate::app::lan_service) fn validate_options_series_and_roles(
    session: &MatchSession,
    launch: &mut SkirmishLaunchSession,
    map_file: &str,
    source_sha256: &str,
    series: Option<&VmceSeriesIdentity>,
    content_sha256: Option<&str>,
    roles: &[VmceRoleRow],
    intermission: Option<&str>,
) -> Result<()> {
    if let Some(previous) = session.vmce_intermission_map() {
        ensure!(
            intermission.is_some_and(|map| map.eq_ignore_ascii_case(previous)),
            "VMCE intermission identity changed"
        );
    }
    if let Some(map) = intermission {
        ensure!(
            launch.mode.class.is_vmce() && series.is_some() && map.eq_ignore_ascii_case(map_file),
            "invalid VMCE intermission identity"
        );
    }
    let lan = session.network_return_context().is_some_and(|context| {
        context.session_mode() == crate::app::match_runtime::session::SessionMode::Lan
    });
    validate_vmce_series(lan, launch, map_file, source_sha256, series, content_sha256)?;
    if launch.mode.class.is_vmce() {
        apply_vmce_roles(launch, roles)?;
    }
    Ok(())
}

/// Resolve the peer's local campaign config before HostSetup. The caller
/// stores the returned config in that transaction and applies the returned
/// display status only after it commits.
pub(in crate::app::lan_service) fn resolve_received_series_config(
    state: &AppState,
    map_file: &str,
    source_sha256: &str,
    identity: Option<&VmceSeriesIdentity>,
) -> (
    Option<crate::app::match_bootstrap::NetworkVmceMapConfig>,
    Option<VmceLobbyStatus>,
) {
    let mut failure = None;
    let resolved = identity.and_then(|identity| {
        let (config, digest) = match crate::app::vmce_campaign::resolve_local_vmce_config(
            state,
            map_file,
            &identity.directory,
            source_sha256,
        ) {
            Ok(value) => value,
            Err(error) => {
                failure = Some(error);
                return None;
            }
        };
        campaign_identity_matches(identity, &config, &digest).then_some(config)
    });
    let status = resolved
        .is_none()
        .then(|| classify_campaign_failure(failure.as_ref()));
    (resolved, status)
}

/// Start or reuse the guest's series verification after HostSetup commits.
/// MatchSession's accepted map config gates whether this revision can scan.
pub(in crate::app::lan_service) fn admit_received_series(
    state: &mut AppState,
    map_file: &str,
    identity: &VmceSeriesIdentity,
    content: &str,
) -> Option<super::VmceSeriesState> {
    let session = &state.match_state.session;
    if session.host()
        || !session
            .network_setup()
            .is_some_and(|launch| launch.mode.class.is_vmce())
        || session.network_vmce_map_config().is_none()
    {
        return None;
    }
    let cached = state
        .match_state
        .lan_service
        .as_ref()
        .map(|service| service.vmce_setup.admission_state_for(session));
    let verification = match cached {
        Some(
            verdict @ (super::VmceSeriesState::Verified { .. }
            | super::VmceSeriesState::Mismatch { .. }
            | super::VmceSeriesState::Failed(_)),
        ) => verdict,
        _ => match local_series_for_identity(state, map_file, identity) {
            Ok(series) => {
                let Some(service) = state.match_state.lan_service.as_mut() else {
                    return Some(super::VmceSeriesState::Idle);
                };
                service.vmce_setup.verify_advertised(&series, content)
            }
            Err(failed) => failed,
        },
    };
    super::record_admission_verdict(state, &verification);
    if let Some(protocol) = state.frontend.network_setup_protocol.as_mut() {
        protocol.set_vmce_lobby_status(series_lobby_status(&verification));
    }
    Some(verification)
}

/// Restart a pending admission scan after its old request-scoped boundary
/// worker was canceled by a same-series material revision.
pub(in crate::app::lan_service) fn ensure_current_series_admission(state: &mut AppState) {
    let Some(identity) = current_series_identity(state) else {
        return;
    };
    let map_file = identity.selected_map_file.clone();
    let content = identity.content_sha256.clone();
    let wire_identity = VmceSeriesIdentity {
        directory: identity.directory,
        sha256: identity.manifest_sha256,
    };
    let _ = admit_received_series(state, &map_file, &wire_identity, &content);
}

/// Project a finished whole-series verdict to the lobby's inline status.
pub(in crate::app::lan_service) fn series_lobby_status(
    verification: &super::VmceSeriesState,
) -> Option<VmceLobbyStatus> {
    match verification {
        super::VmceSeriesState::Mismatch { .. } => Some(VmceLobbyStatus::MapMismatch),
        super::VmceSeriesState::Failed(error) => {
            Some(classify_campaign_failure(Some(&anyhow!("{error}"))))
        }
        super::VmceSeriesState::Verified { .. }
        | super::VmceSeriesState::Pending
        | super::VmceSeriesState::Idle => None,
    }
}

/// Apply a completed coordinator scan to MatchSession and its shell
/// projection. Return true when the verifier may mark this verdict applied.
pub(in crate::app::lan_service) fn reconcile_finished_series_verdict(
    state: &mut AppState,
    verification: &super::VmceSeriesState,
) -> bool {
    let vmce_setup = state
        .match_state
        .session
        .network_setup()
        .is_some_and(|launch| launch.mode.class.is_vmce());
    if state.match_state.session.host()
        || !vmce_setup
        || state
            .match_state
            .session
            .network_vmce_map_config()
            .is_none()
    {
        return true;
    }
    if let Some(protocol) = state.frontend.network_setup_protocol.as_mut() {
        protocol.set_vmce_lobby_status(series_lobby_status(verification));
    }
    let Some(local) = state
        .match_state
        .session
        .network_setup()
        .map(|launch| launch.local_player_index)
    else {
        return false;
    };
    match verification {
        super::VmceSeriesState::Verified { .. } => {
            if state.match_state.session.network_map_status(local) != Some(1) {
                let _ = crate::app::shell_network_setup::apply_local_map_admission(
                    state,
                    crate::app::match_runtime::session::NetworkMapAdmission::Present,
                );
            }
            if let Some(protocol) = state.frontend.network_setup_protocol.as_mut() {
                protocol.clear_vmce_retry();
            }
            unlock_verified_intermission(state);
        }
        super::VmceSeriesState::Mismatch { .. } | super::VmceSeriesState::Failed(_) => {
            report_coop_campaign_unavailable(state);
            let _ = crate::app::shell_network_setup::apply_local_map_admission(
                state,
                crate::app::match_runtime::session::NetworkMapAdmission::Unavailable,
            );
            if let Some(protocol) = state.frontend.network_setup_protocol.as_mut() {
                protocol.allow_vmce_retry();
            }
        }
        super::VmceSeriesState::Pending | super::VmceSeriesState::Idle => return false,
    }
    true
}

/// Re-read local files at each prepare/start boundary. A valid snapshot at
/// options receipt is insufficient if campaign or map files changed later.
pub(in crate::app::lan_service) fn validate_current_coop_campaign(state: &AppState) -> Result<()> {
    let session = &state.match_state.session;
    let launch = session
        .network_setup()
        .ok_or_else(|| anyhow!("LAN setup is missing"))?;
    if !launch.mode.class.is_vmce() {
        return Ok(());
    }
    if state
        .match_state
        .lan_service
        .as_ref()
        .is_some_and(|service| service.preadmitted)
    {
        return Ok(());
    }
    if !session.host() {
        let verification = state.match_state.lan_service.as_ref().map_or(
            super::VmceSeriesState::Idle,
            |service| {
                service
                    .vmce_setup
                    .admission_state_for(&state.match_state.session)
            },
        );
        match verification {
            super::VmceSeriesState::Verified { .. } => {}
            super::VmceSeriesState::Pending => {
                anyhow::bail!("VMCE series verification is pending")
            }
            super::VmceSeriesState::Mismatch { .. } => {
                anyhow::bail!("local VMCE series content differs from the host campaign")
            }
            super::VmceSeriesState::Failed(error) => {
                anyhow::bail!("local VMCE series is unreadable: {error}")
            }
            super::VmceSeriesState::Idle => {
                anyhow::bail!("local VMCE series is not verified")
            }
        }
    }
    let file = launch
        .selected_map_file
        .as_deref()
        .ok_or_else(|| anyhow!("VMCE has no selected map"))?;
    let metadata = session
        .network_map_metadata()
        .ok_or_else(|| anyhow!("VMCE map identity is missing"))?;
    let accepted = session
        .network_vmce_map_config()
        .ok_or_else(|| anyhow!("local VMCE series is not verified"))?;
    accepted.validate_binding(file, &metadata.source_sha256)?;
    let (directory, expected_sha256) = if session.host() {
        crate::app::vmce_campaign::host_vmce_identity(state, file, &metadata.source_sha256)?
    } else {
        let identity = current_series_identity(state)
            .ok_or_else(|| anyhow!("host series identity is missing"))?;
        (identity.directory, identity.manifest_sha256)
    };
    let (local, local_sha256) = crate::app::vmce_campaign::resolve_local_vmce_config(
        state,
        file,
        &directory,
        &metadata.source_sha256,
    )?;
    ensure!(
        local_sha256 == expected_sha256 && local == *accepted,
        "local VMCE series or map changed after options admission"
    );
    Ok(())
}

/// Append the same inline unavailable-campaign notice used by the network
/// setup view.
pub(in crate::app::lan_service) fn report_coop_campaign_unavailable(state: &mut AppState) {
    let text = crate::i18n::tr("VERA:VmceMapMissing");
    crate::app::shell_lan::append_chat(
        state,
        crate::ui::lan_shell::LanChatLine {
            text: text.encode_utf16().collect(),
            color: u32::MAX,
        },
    );
}

/// The intermission stage a guest has earned. `map_status` 1 is the admission
/// proof the whole-series verification produced, so an unlock cannot precede it.
pub(in crate::app::lan_service) fn intermission_unlock_earned(
    session: &MatchSession,
) -> Option<&str> {
    let map = session.vmce_intermission_map()?;
    let launch = session.network_setup()?;
    (session.network_vmce_map_config().is_some()
        && session.network_map_status(launch.local_player_index) == Some(1))
    .then_some(map)
}

pub(in crate::app::lan_service) fn unlock_verified_intermission(state: &AppState) {
    if let Some(map) = intermission_unlock_earned(&state.match_state.session) {
        crate::app::vmce_campaign::unlock_catalog_stage(state, map);
    }
}

pub(in crate::app::lan_service) fn campaign_identity_matches(
    advertised: &VmceSeriesIdentity,
    local: &crate::app::match_bootstrap::NetworkVmceMapConfig,
    local_sha256: &str,
) -> bool {
    advertised.sha256 == local_sha256
        && local.series_directory.as_deref() == Some(advertised.directory.as_str())
        && local.series_sha256.as_deref() == Some(advertised.sha256.as_str())
}

pub(in crate::app::lan_service) fn validate_vmce_series(
    lan: bool,
    launch: &SkirmishLaunchSession,
    map_file: &str,
    source_sha256: &str,
    campaign: Option<&VmceSeriesIdentity>,
    content_sha256: Option<&str>,
) -> Result<()> {
    if launch.mode.class.is_vmce() {
        crate::map::source::validate_network_map_name(map_file)?;
        ensure!(
            source_sha256.is_empty() || crate::map::source::valid_source_sha256(source_sha256),
            "invalid VMCE map identity"
        );
        if let Some(campaign) = campaign {
            ensure!(
                !campaign.directory.is_empty()
                    && std::path::Path::new(&campaign.directory)
                        .components()
                        .count()
                        == 1
                    && crate::map::source::valid_source_sha256(&campaign.sha256),
                "invalid VMCE series identity"
            );
        }
        if lan {
            ensure!(
                campaign.is_some(),
                "VMCE LAN options omit the series identity"
            );
            let content = content_sha256
                .ok_or_else(|| anyhow!("VMCE LAN options omit the series content identity"))?;
            ensure!(
                crate::map::source::valid_source_sha256(content),
                "invalid VMCE series content identity"
            );
        }
    } else {
        ensure!(
            campaign.is_none() && content_sha256.is_none(),
            "LAN non-VMCE options carry VMCE series identity"
        );
    }
    Ok(())
}

pub(in crate::app::lan_service) fn apply_vmce_roles(
    launch: &mut SkirmishLaunchSession,
    roles: &[VmceRoleRow],
) -> Result<()> {
    ensure!(
        launch.mode.class.is_vmce(),
        "LAN VMCE role roster without the VMCE mode"
    );
    ensure!(
        launch.humans.iter().all(|human| !human.is_observer()),
        "VMCE does not admit observers"
    );
    if roles.is_empty() {
        return Ok(());
    }
    // The roster is keyed by the synchronized lobby row (`raw_slot_index`),
    // not by this peer's participant order: LAN admits participants
    // local-first, so the host's and guest's `humans` vectors can order the
    // same people differently.
    let mut by_row = [None; crate::skirmish_launch::NETWORK_PLAYER_SLOT_COUNT];
    for entry in roles {
        ensure!(
            entry.role.native_value() == -2 || (0..4).contains(&entry.role.native_value()),
            "invalid LAN VMCE role"
        );
        let slot = by_row
            .get_mut(usize::from(entry.row))
            .ok_or_else(|| anyhow!("LAN VMCE role row is outside setup rows"))?;
        ensure!(slot.is_none(), "duplicate LAN VMCE role row");
        *slot = Some(entry.role);
    }
    // A guest can receive the options snapshot before its own roster revision,
    // or the roster can lag a republished options packet. Defer the whole
    // assignment in that case: the stored snapshot is re-applied by
    // `reapply_received_options` once the Roster revision arrives.
    let mut bound_rows = 0usize;
    for human in &launch.humans {
        let Some(row) = human.raw_slot_index else {
            continue;
        };
        bound_rows += 1;
        if by_row
            .get(usize::from(row))
            .is_none_or(|role| role.is_none())
        {
            return Ok(());
        }
    }
    if bound_rows != roles.len() {
        return Ok(());
    }
    let mut claimed = [false; 4];
    for human in &mut launch.humans {
        let Some(row) = human.raw_slot_index else {
            continue;
        };
        let Some(role) = by_row[usize::from(row)] else {
            continue;
        };
        let value = role.native_value();
        ensure!(
            value == -2 || (0..4).contains(&value),
            "invalid LAN VMCE role"
        );
        if value >= 0 {
            ensure!(!claimed[value as usize], "duplicate LAN VMCE role");
            claimed[value as usize] = true;
        }
        human.role = role;
    }
    Ok(())
}

fn local_series_for_identity(
    state: &AppState,
    map_file: &str,
    identity: &VmceSeriesIdentity,
) -> std::result::Result<super::super::vmce_series::LocalSeries, super::VmceSeriesState> {
    let series = super::super::vmce_series::read_local_series(state, map_file)
        .map_err(|error| super::VmceSeriesState::Failed(format!("{error:#}")))?;
    if !series.directory.eq_ignore_ascii_case(&identity.directory)
        || series.manifest_sha256() != identity.sha256
    {
        return Err(super::VmceSeriesState::Failed(
            "local VMCE series changed after options admission".to_owned(),
        ));
    }
    Ok(series)
}

pub(in crate::app::lan_service) fn classify_campaign_failure(
    error: Option<&anyhow::Error>,
) -> VmceLobbyStatus {
    let text = error.map(|error| format!("{error:#}")).unwrap_or_default();
    if text.contains("differs")
        || text.contains("does not list")
        || text.contains("does not match")
        || text.contains("changed")
    {
        VmceLobbyStatus::MapMismatch
    } else {
        VmceLobbyStatus::MissingAssets
    }
}
