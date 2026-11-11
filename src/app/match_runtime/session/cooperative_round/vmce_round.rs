//! VMCE-only state and transitions attached to the shared BC6 round.
//!
//! The native roster, readiness, begin/options and acknowledgement protocol
//! stays in the parent `cooperative_round` owner. This child owns the VMCE
//! continuation snapshot and its agreement, retry and page state.

use super::{
    CooperativeRoundState, CooperativeRoundStep, MatchSession, NetworkLoadingRequest, NetworkPhase,
    NetworkSession, RoundParts, round_parts,
};
use crate::app::match_bootstrap::{PreparedNetworkStartup, VmceNextStage, VmceStageIdentity};
use crate::skirmish_launch::{LaunchHumanRole, SkirmishLaunchSession};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VmceRoundState {
    next_stage: Option<VmceNextStage>,
    options_received: bool,
    ack_queued: bool,
    page_victory: bool,
    ending: VmceRoundEnding,
    retry_votes: [bool; 4],
    result_agreement: Option<VmceResultAgreement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VmceResultAgreement {
    entries: [u8; 4],
    nonce: Option<[u8; 16]>,
    proposed: [bool; 8],
    confirmed: [bool; 8],
    proposal_sent: bool,
    confirmation_sent: bool,
    deferred_go: bool,
    early_records: Vec<(
        usize,
        crate::net::lan_messages::cooperative::VmceResultRecord,
    )>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmceRoundEnding {
    Next,
    CampaignEnd,
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VmceRoundStep {
    Assign { ending: VmceRoundEnding },
    Next,
    Retry,
    CampaignEnd,
}

impl VmceRoundState {
    pub(super) fn new(next_stage: Option<VmceNextStage>) -> Self {
        Self {
            next_stage,
            options_received: false,
            ack_queued: false,
            page_victory: false,
            ending: VmceRoundEnding::Next,
            retry_votes: [false; 4],
            result_agreement: None,
        }
    }

    pub(super) fn ending(&self) -> VmceRoundEnding {
        self.ending
    }

    pub(super) fn set_ending(&mut self, ending: VmceRoundEnding) {
        self.ending = ending;
    }

    /// Derived from the immutable next-stage snapshot. It is never stored or
    /// independently updated, so result agreement cannot drift from the map
    /// that retry/next loading will consume.
    fn stage_identity(&self) -> Option<VmceStageIdentity> {
        VmceStageIdentity::of(self.next_stage.as_ref())
    }

    pub(super) fn next_stage(&self) -> Option<&VmceNextStage> {
        self.next_stage.as_ref()
    }

    pub(super) fn next_stage_cloned(&self) -> Option<VmceNextStage> {
        self.next_stage.clone()
    }

    pub(super) fn page_victory(&self) -> bool {
        self.page_victory
    }

    pub(super) fn set_page_victory(&mut self, victory: bool) {
        self.page_victory = victory;
    }

    pub(super) fn retry_voted(&self, row: usize) -> bool {
        self.retry_votes.get(row).copied().unwrap_or(false)
    }

    pub(super) fn stage_identity_matches(&self, advertised: &VmceStageIdentity) -> bool {
        self.stage_identity().as_ref() == Some(advertised)
    }

    pub(super) fn has_stage(&self) -> bool {
        self.next_stage.is_some()
    }

    pub(super) fn note_retry_vote(&mut self, row: usize) {
        if let Some(vote) = self.retry_votes.get_mut(row) {
            *vote = true;
        }
    }

    pub(super) fn clear_retry_vote(&mut self, row: usize) {
        if let Some(vote) = self.retry_votes.get_mut(row) {
            *vote = false;
        }
    }

    pub(super) fn acknowledge_after_begin(&mut self, host: bool) -> bool {
        if host || !self.options_received || self.ack_queued {
            return false;
        }
        self.ack_queued = true;
        true
    }

    pub(super) fn acknowledge_after_options(&mut self, begin: bool, host: bool) -> bool {
        self.options_received = true;
        if host || !begin || self.ack_queued {
            return false;
        }
        self.ack_queued = true;
        true
    }

    pub(super) fn options_received(&self) -> bool {
        self.options_received
    }

    pub(super) fn ack_queued(&self) -> bool {
        self.ack_queued
    }

    pub(super) fn result_nonce(&self) -> Option<[u8; 16]> {
        self.result_agreement.as_ref()?.nonce
    }

    fn result_agreement_initialized(&self) -> bool {
        self.result_agreement.is_some()
    }

    pub(super) fn result_step(&self) -> VmceRoundStep {
        match self.ending {
            VmceRoundEnding::Next => VmceRoundStep::Next,
            VmceRoundEnding::CampaignEnd => VmceRoundStep::CampaignEnd,
            VmceRoundEnding::Retry => VmceRoundStep::Retry,
        }
    }

    pub(super) fn accepted(&self, outcome: Option<bool>) -> bool {
        self.ending == VmceRoundEnding::Next && outcome == Some(true)
    }

    pub(super) fn result_stage_digest(&self, nonce: [u8; 16]) -> [u8; 32] {
        vmce_result_stage_digest(nonce, self.stage_identity().as_ref())
    }

    pub(super) fn confirmed_count(&self) -> usize {
        self.result_agreement.as_ref().map_or(0, |agreement| {
            agreement
                .confirmed
                .iter()
                .filter(|confirmed| **confirmed)
                .count()
        })
    }
}

pub(super) fn vmce_result_stage_digest(
    nonce: [u8; 16],
    stage: Option<&VmceStageIdentity>,
) -> [u8; 32] {
    let mut sha = crate::util::sha256::Sha256::new();
    sha.update(b"vera-vmce-result-stage\0v1\0");
    sha.update(&nonce);
    match stage {
        None => sha.update(&[0]),
        Some(stage) => {
            sha.update(&[1]);
            let file = stage.map_file.as_bytes();
            sha.update(&(file.len() as u64).to_le_bytes());
            sha.update(file);
            sha.update(stage.source_sha256.as_bytes());
            sha.update(stage.effective_digest.as_bytes());
            sha.update(stage.config_sha256.as_bytes());
        }
    }
    sha.finalize()
}

/// VMCE's next-mission seed: peers derive it from the prior agreed seed and
/// their locally verified stage identity.
pub(super) fn next_stage_seed(previous: u32, next: &VmceNextStage) -> u32 {
    let digest = crate::util::sha256::sha256_hex(
        format!(
            "vera-vmce-next-seed\n{previous}\n{}\n{}\n{}",
            next.map_file, next.config.source_sha256, next.effective_digest
        )
        .as_bytes(),
    );
    u32::from_str_radix(&digest[..8], 16).expect("sha256 hex prefix")
}

impl MatchSession {
    /// VMCE enters BC6 explicitly; a generic Cooperative round never gains
    /// VMCE behavior merely because its launch happens to carry that mode.
    pub(crate) fn enter_vmce_round(
        &mut self,
        next: Option<VmceNextStage>,
        can_continue: bool,
        rows: usize,
    ) -> anyhow::Result<()> {
        let next_map = next.as_ref().map(|stage| stage.map_file.clone());
        self.enter_cooperative_round(next_map, can_continue, rows)?;

        let host = self.host;
        let NetworkSession {
            loading_requests,
            phase,
            ..
        } = self
            .network
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
        let NetworkPhase::CooperativeRound { launch, round, .. } = phase else {
            anyhow::bail!("VMCE requires a cooperative round");
        };
        anyhow::ensure!(round.vmce.is_none(), "VMCE round already initialized");

        let vmce = VmceRoundState::new(next);
        if host {
            // VMCE's authored host is ready from the start; only its Continue
            // starts a next stage.
            round.local_ready = true;
            if let Some(row) = launch
                .humans
                .get(launch.local_player_index)
                .and_then(|human| human.raw_slot_index)
            {
                round.ready[usize::from(row)] = true;
            }
            loading_requests.push(NetworkLoadingRequest::CooperativeReady);
        }
        if let Some(stage) = vmce.stage_identity() {
            loading_requests.push(NetworkLoadingRequest::CooperativeStage(stage));
        } else {
            loading_requests.push(NetworkLoadingRequest::CooperativeStageAbsent);
        }
        round.vmce = Some(vmce);
        Ok(())
    }

    pub(crate) fn enter_vmce_round_ending(
        &mut self,
        next: Option<VmceNextStage>,
        can_continue: bool,
        ending: VmceRoundEnding,
        rows: usize,
    ) -> anyhow::Result<()> {
        self.enter_vmce_round(next, can_continue, rows)?;
        let RoundParts { round, .. } = round_parts(self)?;
        round
            .vmce
            .as_mut()
            .expect("VMCE entry initializes VMCE state")
            .set_ending(ending);
        Ok(())
    }

    pub(crate) fn vmce_round_next_stage_map(&self) -> Option<&str> {
        Some(
            self.cooperative_round()?
                .vmce
                .as_ref()?
                .next_stage()?
                .map_file
                .as_str(),
        )
    }

    pub(crate) fn set_cooperative_round_page_victory(&mut self, victory: bool) {
        if let Some(round) = self
            .network
            .as_mut()
            .and_then(|network| match &mut network.phase {
                NetworkPhase::CooperativeRound { round, .. } => Some(round),
                _ => None,
            })
        {
            if let Some(vmce) = round.vmce.as_mut() {
                vmce.set_page_victory(victory);
            }
        }
    }

    /// Reopen the existing VMCE roster on the resolved next map. Transport
    /// handoff must complete before this is exposed as an editable room.
    pub(crate) fn enter_vmce_intermission_setup(&mut self) -> anyhow::Result<VmceNextStage> {
        let network = self
            .network
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
        let NetworkPhase::CooperativeRound {
            previous,
            launch,
            connected,
            round,
        } = &network.phase
        else {
            anyhow::bail!("VMCE intermission requires a completed round");
        };
        let vmce = round
            .vmce
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("VMCE round state is absent"))?;
        anyhow::ensure!(
            vmce.ending() == VmceRoundEnding::Next && round.outcome == Some(true),
            "VMCE round did not advance"
        );
        let stage = vmce
            .next_stage_cloned()
            .ok_or_else(|| anyhow::anyhow!("VMCE next stage is absent"))?;
        let next_seed = next_stage_seed(previous.seed.value, &stage);
        let local = launch.local_player_index;
        anyhow::ensure!(
            connected.contains(&local),
            "local participant left the round"
        );
        let mut next_launch = launch.clone();
        next_launch.humans = launch
            .humans
            .iter()
            .enumerate()
            .filter(|(index, _)| connected.contains(index))
            .map(|(_, human)| human.clone())
            .collect();
        next_launch.local_player_index = connected.iter().filter(|index| **index < local).count();
        next_launch.selected_map_file = Some(stage.map_file.clone());
        for human in &mut next_launch.humans {
            human.role = LaunchHumanRole::Player { group: -2 };
        }
        next_launch.rebuild_house_roster();
        let names = next_launch
            .humans
            .iter()
            .map(|human| human.name.clone())
            .collect::<Vec<_>>();
        self.accept_network_setup(next_launch, self.host, None, true)?;
        if self.host {
            let local = self
                .network_setup()
                .expect("accepted setup")
                .local_player_index;
            self.set_network_player_ready(local, true)?;
        }
        let network = self.network.as_mut().expect("accepted setup has a network");
        network.endpoints.retain(|name, _| names.contains(name));
        network.map_metadata = Some(stage.metadata.clone());
        network.vmce_map_config = Some(stage.config.clone());
        network.vmce_intermission_map = Some(stage.map_file.clone());
        network.local_map_source = Some(stage.local_map_source.clone());
        network.local_map_effective_digest = Some(stage.effective_digest.clone());
        network.local_random_map_preview = None;
        network.proposed_seed = Some(next_seed);
        network.max_ahead_frames = None;
        Ok(stage)
    }
}

fn vmce_all_seen(connected: &[usize], launch: &SkirmishLaunchSession, seen: &[bool; 8]) -> bool {
    !connected.is_empty()
        && connected.iter().all(|source| {
            launch
                .humans
                .get(*source)
                .and_then(|human| human.raw_slot_index)
                .is_some_and(|slot| slot < 4 && seen[usize::from(slot)])
        })
}

impl MatchSession {
    pub(crate) fn vmce_result_agreement_initialized(&self) -> bool {
        self.cooperative_round()
            .and_then(|round| round.vmce.as_ref())
            .is_some_and(VmceRoundState::result_agreement_initialized)
    }

    pub(crate) fn vmce_result_nonce(&self) -> Option<[u8; 16]> {
        self.cooperative_round()?.vmce.as_ref()?.result_nonce()
    }

    pub(crate) fn initialize_vmce_result_agreement(
        &mut self,
        entries: [u8; 4],
        nonce: Option<[u8; 16]>,
    ) -> anyhow::Result<()> {
        let host = self.host;
        let RoundParts { launch, round, .. } = round_parts(self)?;
        let vmce = round
            .vmce
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no VMCE result round"))?;
        anyhow::ensure!(
            !vmce.result_agreement_initialized(),
            "invalid VMCE result agreement initialization"
        );
        anyhow::ensure!(
            host == nonce.is_some(),
            "only the host chooses the result nonce"
        );
        let local = launch
            .humans
            .get(launch.local_player_index)
            .and_then(|human| human.raw_slot_index)
            .filter(|slot| *slot < 4)
            .ok_or_else(|| anyhow::anyhow!("VMCE local result slot is absent"))?;
        anyhow::ensure!(
            entries[usize::from(local)] != 0xff,
            "VMCE local result is absent"
        );
        vmce.result_agreement = Some(VmceResultAgreement {
            entries,
            nonce,
            proposed: [false; 8],
            confirmed: [false; 8],
            proposal_sent: false,
            confirmation_sent: false,
            deferred_go: false,
            early_records: Vec::new(),
        });
        Ok(())
    }

    /// Next all-peer agreement record, retained until the reliable FIFO
    /// accepts it. Host starts a fresh nonce; guests echo that nonce.
    pub(crate) fn vmce_result_next_record(
        &self,
    ) -> Option<crate::net::lan_messages::cooperative::VmceResultRecord> {
        let NetworkPhase::CooperativeRound {
            connected,
            launch,
            round,
            ..
        } = &self.network.as_ref()?.phase
        else {
            return None;
        };
        let vmce = round.vmce.as_ref()?;
        let agreement = vmce.result_agreement.as_ref()?;
        let nonce = agreement.nonce?;
        let confirmed = if !agreement.proposal_sent {
            false
        } else if !agreement.confirmation_sent
            && vmce_all_seen(connected, launch, &agreement.proposed)
        {
            true
        } else {
            return None;
        };
        Some(crate::net::lan_messages::cooperative::VmceResultRecord {
            nonce,
            entries: agreement.entries,
            confirmed,
            stage_digest: vmce.result_stage_digest(nonce),
        })
    }

    pub(crate) fn vmce_result_record_queued(&mut self, confirmed: bool) -> anyhow::Result<()> {
        let RoundParts { launch, round, .. } = round_parts(self)?;
        let row = launch
            .humans
            .get(launch.local_player_index)
            .and_then(|human| human.raw_slot_index)
            .filter(|row| *row < 4)
            .ok_or_else(|| anyhow::anyhow!("VMCE local result slot is absent"))?
            as usize;
        let agreement = round
            .vmce
            .as_mut()
            .and_then(|vmce| vmce.result_agreement.as_mut())
            .ok_or_else(|| anyhow::anyhow!("VMCE result agreement is absent"))?;
        if confirmed {
            agreement.confirmation_sent = true;
            agreement.confirmed[row] = true;
        } else {
            agreement.proposal_sent = true;
            agreement.proposed[row] = true;
        }
        Ok(())
    }

    pub(crate) fn vmce_result_received(
        &mut self,
        source: usize,
        record: crate::net::lan_messages::cooperative::VmceResultRecord,
    ) -> anyhow::Result<()> {
        let host = self.host;
        let RoundParts {
            connected,
            launch,
            round,
            ..
        } = round_parts(self)?;
        anyhow::ensure!(
            round.vmce.is_some() && connected.contains(&source),
            "result sender is not an active VMCE participant"
        );
        let row = launch
            .humans
            .get(source)
            .and_then(|human| human.raw_slot_index)
            .filter(|row| *row < 4)
            .ok_or_else(|| anyhow::anyhow!("result sender has no VMCE slot"))?
            as usize;
        let vmce = round.vmce.as_mut().expect("checked VMCE round");
        let stage_digest = vmce_result_stage_digest(record.nonce, vmce.stage_identity().as_ref());
        let agreement = vmce
            .result_agreement
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("VMCE result agreement is absent"))?;
        anyhow::ensure!(
            record.valid() && record.entries == agreement.entries,
            "VMCE players disagree on terminal results"
        );
        anyhow::ensure!(
            record.stage_digest == stage_digest,
            "VMCE players disagree on the next stage"
        );
        if agreement.nonce.is_none() {
            if !host && row != 0 {
                anyhow::ensure!(
                    agreement.early_records.len() < 8,
                    "too many early VMCE result records"
                );
                agreement.early_records.push((row, record));
                return Ok(());
            }
            anyhow::ensure!(
                !host && row == 0 && !record.confirmed,
                "only the host can initiate result agreement"
            );
            agreement.nonce = Some(record.nonce);
            for (early_row, early) in std::mem::take(&mut agreement.early_records) {
                if early.nonce == record.nonce {
                    agreement.proposed[early_row] = true;
                    if early.confirmed {
                        agreement.confirmed[early_row] = true;
                    }
                }
            }
        }
        anyhow::ensure!(
            agreement.nonce == Some(record.nonce),
            "VMCE result agreement nonce differs"
        );
        agreement.proposed[row] = true;
        if record.confirmed {
            agreement.confirmed[row] = true;
        }
        Ok(())
    }

    pub(crate) fn vmce_result_take_deferred_go(
        &mut self,
    ) -> anyhow::Result<Option<CooperativeRoundStep>> {
        let RoundParts {
            connected,
            launch,
            round,
            ..
        } = round_parts(self)?;
        let agreed = result_agreed(connected, launch, round);
        let Some(vmce) = round.vmce.as_mut() else {
            return Ok(None);
        };
        if round.outcome.is_some()
            || !agreed
            || !vmce
                .result_agreement
                .as_ref()
                .is_some_and(|agreement| agreement.deferred_go)
        {
            return Ok(None);
        }
        round.outcome = Some(true);
        Ok(Some(result_step(round)))
    }
}

pub(super) fn result_agreed(
    connected: &[usize],
    launch: &SkirmishLaunchSession,
    round: &CooperativeRoundState,
) -> bool {
    round.vmce.as_ref().is_some_and(|vmce| {
        vmce.result_agreement.as_ref().is_some_and(|agreement| {
            agreement.confirmation_sent
                && vmce_all_seen(connected, launch, &agreement.proposed)
                && vmce_all_seen(connected, launch, &agreement.confirmed)
        })
    })
}

pub(super) fn result_step(round: &CooperativeRoundState) -> CooperativeRoundStep {
    CooperativeRoundStep::VmceRound(
        round
            .vmce
            .as_ref()
            .expect("VMCE step belongs to a VMCE round")
            .result_step(),
    )
}

fn retry_guests_voted(
    launch: &SkirmishLaunchSession,
    connected: &[usize],
    vmce: &VmceRoundState,
) -> bool {
    connected.iter().all(|index| {
        let Some(human) = launch.humans.get(*index) else {
            return true;
        };
        if human.is_observer() {
            return true;
        }
        match human.raw_slot_index {
            Some(0) | None => true,
            Some(row) => vmce.retry_voted(usize::from(row)),
        }
    })
}

pub(super) fn continue_enabled(
    host: bool,
    connected: &[usize],
    launch: &SkirmishLaunchSession,
    round: &CooperativeRoundState,
) -> bool {
    let Some(vmce) = round.vmce.as_ref() else {
        return false;
    };
    if round.outcome.is_some() || (host && !result_agreed(connected, launch, round)) {
        return false;
    }
    if !round.can_continue {
        return true;
    }
    if connected.len() < round.rows {
        return false;
    }
    if host {
        (vmce.ending() == VmceRoundEnding::Retry || round.all_ready()) && !round.assigned
    } else {
        !round.local_ready
    }
}

pub(super) fn continue_round(
    host: bool,
    online: bool,
    requests: &mut Vec<NetworkLoadingRequest>,
    connected: &[usize],
    launch: &mut SkirmishLaunchSession,
    round: &mut CooperativeRoundState,
) -> anyhow::Result<CooperativeRoundStep> {
    let vmce = round
        .vmce
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("not a VMCE round"))?;
    anyhow::ensure!(round.outcome.is_none(), "the result dialog has closed");
    anyhow::ensure!(
        !host || result_agreed(connected, launch, round),
        "VMCE result agreement is incomplete"
    );
    let ending = vmce.ending();
    if !round.can_continue {
        round.outcome = Some(false);
        requests.push(NetworkLoadingRequest::CooperativeLeave);
        return Ok(CooperativeRoundStep::Exit(false));
    }
    if host && round.rows == 1 {
        round.outcome = Some(true);
        return Ok(result_step(round));
    }
    if ending == VmceRoundEnding::Retry {
        if host {
            return Ok(CooperativeRoundStep::VmceRound(VmceRoundStep::Assign {
                ending,
            }));
        }
        return Ok(CooperativeRoundStep::None);
    }
    if online {
        round.outcome = Some(false);
        return Ok(CooperativeRoundStep::ContinueInRoom);
    }
    if host {
        anyhow::ensure!(
            round.all_ready() && !round.assigned,
            "the host Continue waits for every guest"
        );
        log::debug!("VMCE round: host Continue pressed; starting the next stage");
        return Ok(CooperativeRoundStep::VmceRound(VmceRoundStep::Assign {
            ending,
        }));
    }
    let local = launch.local_player_index;
    round.local_ready = true;
    log::debug!("VMCE round: guest ready pressed");
    if let Some(row) = launch
        .humans
        .get(local)
        .and_then(|human| human.raw_slot_index)
    {
        round.ready[usize::from(row)] = true;
    }
    requests.push(NetworkLoadingRequest::CooperativeReady);
    Ok(CooperativeRoundStep::None)
}

pub(super) fn retry_state(session: &MatchSession) -> Option<(bool, bool, bool)> {
    let NetworkPhase::CooperativeRound {
        connected,
        launch,
        round,
        ..
    } = &session.network.as_ref()?.phase
    else {
        return None;
    };
    let vmce = round.vmce.as_ref()?;
    if vmce.ending() != VmceRoundEnding::Retry || round.outcome.is_some() {
        return None;
    }
    if session.host() {
        let voted =
            retry_guests_voted(launch, connected, vmce) && result_agreed(connected, launch, round);
        Some((true, voted, voted))
    } else {
        let voted = launch
            .humans
            .get(launch.local_player_index)
            .and_then(|human| human.raw_slot_index)
            .is_some_and(|row| vmce.retry_voted(usize::from(row)));
        Some((false, voted, !voted))
    }
}

fn retry_vote(session: &mut MatchSession) -> anyhow::Result<CooperativeRoundStep> {
    anyhow::ensure!(!session.host(), "the host does not cast a retry vote");
    let RoundParts {
        requests,
        launch,
        round,
        ..
    } = round_parts(session)?;
    let vmce = round
        .vmce
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("not a VMCE round"))?;
    anyhow::ensure!(
        vmce.ending() == VmceRoundEnding::Retry,
        "not the VMCE retry page"
    );
    if round.outcome.is_some() {
        return Ok(CooperativeRoundStep::None);
    }
    let Some(row) = launch
        .humans
        .get(launch.local_player_index)
        .and_then(|human| human.raw_slot_index)
        .map(usize::from)
        .filter(|row| *row < 4)
    else {
        anyhow::bail!("the local guest has no raw slot");
    };
    if vmce.retry_voted(row) {
        return Ok(CooperativeRoundStep::None);
    }
    vmce.note_retry_vote(row);
    requests.push(NetworkLoadingRequest::CooperativeRetryVote);
    Ok(CooperativeRoundStep::None)
}

fn retry_start(
    session: &mut MatchSession,
    difficulty: i32,
) -> anyhow::Result<CooperativeRoundStep> {
    anyhow::ensure!(session.host(), "only the host starts the retry");
    {
        let RoundParts {
            connected,
            launch,
            round,
            ..
        } = round_parts(session)?;
        let vmce = round
            .vmce
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not a VMCE round"))?;
        anyhow::ensure!(
            vmce.ending() == VmceRoundEnding::Retry,
            "not the VMCE retry page"
        );
        anyhow::ensure!(
            retry_guests_voted(launch, connected, vmce),
            "the host retry waits for every online guest"
        );
    }
    session.continue_cooperative_round(difficulty)
}

fn set_retry_votes_for_diagnostics(
    session: &mut MatchSession,
    votes: [bool; 4],
) -> anyhow::Result<()> {
    let RoundParts { round, .. } = round_parts(session)?;
    let vmce = round
        .vmce
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("not a VMCE round"))?;
    vmce.retry_votes = votes;
    Ok(())
}

impl MatchSession {
    /// Retry-page projection used by the app and capture diagnostics.
    pub(crate) fn cooperative_round_retry_vote(&mut self) -> anyhow::Result<CooperativeRoundStep> {
        retry_vote(self)
    }

    /// Host retry start after the child round confirms all guest votes.
    pub(crate) fn cooperative_round_retry_start(
        &mut self,
        difficulty: i32,
    ) -> anyhow::Result<CooperativeRoundStep> {
        retry_start(self, difficulty)
    }

    /// Set fixture votes for the synthetic defeat-page capture.
    pub(crate) fn cooperative_round_set_retry_votes_for_diagnostics(
        &mut self,
        votes: [bool; 4],
    ) -> anyhow::Result<()> {
        set_retry_votes_for_diagnostics(self, votes)
    }
}

pub(super) fn assignment_allowed(
    connected: &[usize],
    launch: &SkirmishLaunchSession,
    round: &CooperativeRoundState,
) -> bool {
    result_agreed(connected, launch, round)
}

pub(super) fn stage_event(
    round: &mut CooperativeRoundState,
    requests: &mut Vec<NetworkLoadingRequest>,
    advertised: Option<&VmceStageIdentity>,
) -> CooperativeRoundStep {
    let matches = round.vmce.as_ref().is_some_and(|vmce| match advertised {
        Some(stage) => vmce.stage_identity_matches(stage),
        None => !vmce.has_stage(),
    });
    if matches {
        return CooperativeRoundStep::None;
    }
    log::warn!(
        "VMCE next-stage identity mismatch; returning both peers to the lobby instead of loading"
    );
    round.outcome = Some(false);
    requests.push(NetworkLoadingRequest::CooperativeStageRejected);
    CooperativeRoundStep::Exit(false)
}

pub(super) fn stage_rejected(
    round: &mut CooperativeRoundState,
    requests: &mut Vec<NetworkLoadingRequest>,
) -> CooperativeRoundStep {
    round.outcome = Some(false);
    requests.push(NetworkLoadingRequest::CooperativeLeave);
    CooperativeRoundStep::Exit(false)
}

pub(super) fn retry_vote_received(round: &mut CooperativeRoundState, row: Option<usize>) {
    if let (Some(vmce), Some(row)) = (round.vmce.as_mut(), row.filter(|row| *row < 4)) {
        vmce.note_retry_vote(row);
    }
}

pub(super) fn peer_left(
    round: &mut CooperativeRoundState,
    row: Option<usize>,
    requests: &mut Vec<NetworkLoadingRequest>,
) -> Option<CooperativeRoundStep> {
    let vmce = round.vmce.as_mut()?;
    if let Some(row) = row.filter(|row| *row < 4) {
        vmce.clear_retry_vote(row);
    }
    round.outcome = Some(false);
    requests.push(NetworkLoadingRequest::CooperativeLeave);
    Some(CooperativeRoundStep::Exit(false))
}

pub(super) fn peer_ready(round: &CooperativeRoundState, source: usize, host: bool) {
    if round.vmce.is_some() {
        log::debug!(
            "VMCE round: PeerReady source={source}, all_ready={}, host={host}",
            round.all_ready()
        );
    }
}

pub(super) fn begin_received(round: &mut CooperativeRoundState, host: bool) -> bool {
    round
        .vmce
        .as_mut()
        .is_some_and(|vmce| vmce.acknowledge_after_begin(host))
}

pub(super) fn ignore_duplicate_options(round: &CooperativeRoundState) -> bool {
    round
        .vmce
        .as_ref()
        .is_some_and(VmceRoundState::options_received)
}

pub(super) fn options_received(round: &mut CooperativeRoundState, host: bool) -> bool {
    round
        .vmce
        .as_mut()
        .is_some_and(|vmce| vmce.acknowledge_after_options(round.begin, host))
}

pub(super) fn ack_allowed(
    host: bool,
    connected: &[usize],
    launch: &SkirmishLaunchSession,
    round: &CooperativeRoundState,
    source: usize,
) -> bool {
    let Some(vmce) = round.vmce.as_ref() else {
        return true;
    };
    host && round.assigned
        && result_agreed(connected, launch, round)
        && source != launch.local_player_index
        && connected.contains(&source)
        && vmce.result_nonce().is_some()
}

pub(super) fn acknowledge_all(
    round: &mut CooperativeRoundState,
    requests: &mut Vec<NetworkLoadingRequest>,
) -> CooperativeRoundStep {
    round.outcome = Some(true);
    let nonce = round
        .vmce
        .as_ref()
        .and_then(VmceRoundState::result_nonce)
        .expect("an agreed VMCE round has a nonce");
    requests.push(NetworkLoadingRequest::CooperativeVmceGo { nonce });
    result_step(round)
}

pub(super) fn go_received(
    host: bool,
    connected: &[usize],
    launch: &SkirmishLaunchSession,
    round: &mut CooperativeRoundState,
) -> Option<CooperativeRoundStep> {
    let vmce = round.vmce.as_ref()?;
    if host || !round.begin || !vmce.options_received() || !vmce.ack_queued() {
        return Some(CooperativeRoundStep::None);
    }
    if !result_agreed(connected, launch, round) {
        if let Some(agreement) = round
            .vmce
            .as_mut()
            .and_then(|vmce| vmce.result_agreement.as_mut())
        {
            agreement.deferred_go = true;
        }
        return Some(CooperativeRoundStep::None);
    }
    round.outcome = Some(true);
    Some(result_step(round))
}

pub(super) struct FreezeData {
    pub(super) seed: u32,
    retry_nonce: Option<[u8; 16]>,
    previous_houses: Option<crate::skirmish_launch::VmceHouseAssignments>,
    next_houses: Option<crate::skirmish_launch::VmceHouseAssignments>,
    next_stage: Option<VmceNextStage>,
    retry_map_identity: Option<(
        Option<crate::app::match_bootstrap::NetworkVmceMapConfig>,
        Option<crate::map::source::LoadedMapSource>,
        Option<String>,
    )>,
}

impl FreezeData {
    pub(super) fn apply_network_identity(&self, network: &mut NetworkSession) {
        if let Some(next) = self.next_stage.as_ref() {
            network.vmce_map_config = Some(next.config.clone());
            network.map_metadata = Some(next.metadata.clone());
            network.local_map_source = Some(next.local_map_source.clone());
            network.local_map_effective_digest = Some(next.effective_digest.clone());
        } else if let Some((config, source, digest)) = self.retry_map_identity.as_ref() {
            if let Some(config) = config {
                network.vmce_map_config = Some(config.clone());
            }
            network.local_map_source = source.clone();
            network.local_map_effective_digest = digest.clone();
        }
    }

    pub(super) fn apply_startup(
        &self,
        mut prepared: PreparedNetworkStartup,
    ) -> PreparedNetworkStartup {
        if let Some(nonce) = self.retry_nonce {
            prepared = prepared.with_vmce_retry_attempt(nonce);
        }
        if let Some(houses) = self
            .next_houses
            .clone()
            .or_else(|| self.previous_houses.clone())
        {
            prepared = prepared.with_vmce_houses(houses);
        }
        if let Some(next) = self.next_stage.as_ref() {
            prepared = prepared
                .with_local_map_source(
                    Some(next.local_map_source.clone()),
                    Some(next.effective_digest.clone()),
                )
                .with_vmce_map_config(next.config.clone());
        } else if let Some((config, source, digest)) = self.retry_map_identity.clone() {
            if let Some(config) = config {
                prepared = prepared.with_vmce_map_config(config);
            }
            prepared = prepared.with_local_map_source(source, digest);
        }
        prepared
    }
}

pub(super) fn prepare_freeze(
    previous: &PreparedNetworkStartup,
    launch: &SkirmishLaunchSession,
    round: &CooperativeRoundState,
) -> anyhow::Result<FreezeData> {
    let vmce = round.vmce.as_ref();
    let retry_nonce = if vmce.is_some_and(|vmce| vmce.ending() == VmceRoundEnding::Retry) {
        Some(
            vmce.and_then(VmceRoundState::result_nonce)
                .ok_or_else(|| anyhow::anyhow!("VMCE retry has no result identity"))?,
        )
    } else {
        None
    };
    let next_stage = vmce.and_then(VmceRoundState::next_stage_cloned);
    let seed = match next_stage.as_ref() {
        Some(next) => next_stage_seed(previous.seed.value, next),
        None => previous.seed.value,
    };
    let previous_houses = previous.vmce_houses().cloned();
    // Retry reopens the same frozen stage. Capture the admitted map identity
    // before network fields are changed so the rebuilt start inherits it
    // without re-reading campaign data or resolving a different source.
    let retry_map_identity = next_stage.is_none().then(|| {
        (
            previous.vmce_map_config().cloned(),
            previous.local_map_source().cloned(),
            previous.effective_map_digest().map(str::to_owned),
        )
    });
    let next_houses = if let Some(next) = next_stage.as_ref() {
        // Resolve the new map's own PlayerN/Houses binding while preserving
        // the lobby difficulty chosen for this campaign.
        let mut houses = crate::app::match_bootstrap::resolve_vmce_houses_with_config(
            &next.map_ini,
            &next.map_file,
            &next.config.source_sha256,
            &next.config,
            launch,
            seed,
        )?;
        if let Some(difficulty) = previous_houses.as_ref().map(|houses| houses.difficulty) {
            houses.difficulty = difficulty;
        }
        Some(houses)
    } else {
        None
    };
    Ok(FreezeData {
        seed,
        retry_nonce,
        previous_houses,
        next_houses,
        next_stage,
        retry_map_identity,
    })
}
