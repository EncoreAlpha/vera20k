//! VMCE's own result phase: the settlement, retry, next-stage and campaign-end
//! state, its agreement and its wire decisions.
//!
//! No Cooperative BC6 state, event or UI type is referenced here. Roster,
//! transport and base UI stay VERA's; the result round itself is VMCE's.

use super::{MatchSession, NetworkLoadingRequest, NetworkPhase, NetworkSession};
use crate::app::match_bootstrap::{PreparedNetworkStartup, VmceNextStage, VmceStageIdentity};
use crate::skirmish_launch::{LaunchHumanRole, LaunchStartPosition, SkirmishLaunchSession};
use std::sync::Arc;

/// The VMCE result phase in the session's network visit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VmceResultPhase {
    /// Per raw slot: this peer accepted the result page's Continue/Retry.
    ready: [bool; 8],
    local_ready: bool,
    /// The host has committed the result and sent the proposal.
    assigned: bool,
    /// A guest has received the host's proposal.
    begin: bool,
    can_continue: bool,
    rows: usize,
    outcome: Option<bool>,
    vmce: Option<VmceRoundState>,
}

impl VmceResultPhase {
    fn all_ready(&self) -> bool {
        self.ready.iter().all(|ready| *ready)
    }

    pub(crate) fn vmce_active(&self) -> bool {
        true
    }

    pub(crate) fn vmce_ending(&self) -> Option<VmceRoundEnding> {
        self.vmce.as_ref().map(VmceRoundState::ending)
    }

    pub(crate) fn page_victory(&self) -> bool {
        self.vmce.as_ref().is_some_and(VmceRoundState::page_victory)
    }

    pub(crate) fn vmce_next_accepted(&self) -> bool {
        self.vmce
            .as_ref()
            .is_some_and(|vmce| vmce.accepted(self.outcome))
    }

    pub(crate) fn awaiting_assignment(&self) -> bool {
        // Before the host commits (or the guest consumes the host's Go) the
        // result page is waiting on the other peer; that wait is bounded by
        // the adapter's assignment/Go deadline.
        self.outcome.is_none()
    }

    pub(crate) fn can_continue(&self) -> bool {
        self.can_continue
    }

    pub(crate) fn outcome_committed(&self) -> bool {
        self.outcome.is_some()
    }

    pub(crate) fn local_ready(&self) -> bool {
        self.local_ready
    }
}

/// The VMCE result phase's fields, split from the visit so a step can queue
/// requests while it edits the phase.
struct VmceResultParts<'a> {
    requests: &'a mut Vec<NetworkLoadingRequest>,
    connected: &'a mut Vec<usize>,
    launch: &'a mut SkirmishLaunchSession,
    round: &'a mut VmceResultPhase,
}

fn vmce_result_parts(session: &mut MatchSession) -> anyhow::Result<VmceResultParts<'_>> {
    let NetworkSession {
        loading_requests,
        phase,
        ..
    } = session
        .network
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
    let NetworkPhase::VmceResult {
        connected,
        launch,
        round,
        ..
    } = phase
    else {
        anyhow::bail!("no VMCE result round");
    };
    Ok(VmceResultParts {
        requests: loading_requests,
        connected,
        launch,
        round,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VmceRoundState {
    next_stage: Option<VmceNextStage>,
    page_victory: bool,
    ending: VmceRoundEnding,
    retry_votes: [bool; 4],
    retry_acks: [bool; 4],
    /// This guest consumed the retry Go and is waiting for the host's
    /// delivery commit before it reloads.
    retry_go_pending: bool,
    /// Host: per raw row, the guest reported that its application applied the
    /// retry delivery commit. A transport acknowledgement is not this proof.
    retry_commit_acks: [bool; 4],
    /// Guest: this application applied the host's retry commit. The reload
    /// still waits for the host's RetryStart.
    retry_commit_received: bool,
    /// Guest: the host's RetryStart was consumed; a duplicate never reloads
    /// the retry stage twice.
    retry_started: bool,
    /// A bounded handoff failure the app must report when the round exits.
    failure: Option<VmceRoundFailure>,
    result_agreement: Option<VmceResultAgreement>,
}

/// A result-round handoff that cannot complete. Its i18n key is the visible
/// reason the round owner shows before returning both peers to the LAN lobby.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmceRoundFailure {
    /// A round message could not be queued or the transport failed.
    Delivery,
    /// A connected peer stopped answering the result-round application
    /// heartbeat for the bounded window.
    PeerUnresponsive,
}

impl VmceRoundFailure {
    pub(crate) const fn i18n_key(self) -> &'static str {
        match self {
            Self::Delivery => "VERA:VmceRoundSendFailed",
            Self::PeerUnresponsive => "VERA:VmcePeerUnresponsive",
        }
    }
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
    early_records: Vec<(usize, crate::net::lan_messages::vmce::VmceResultRecord)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VmceRoundEnding {
    Next,
    CampaignEnd,
    Retry,
}

/// A decoded VMCE result message. VMCE owns this event vocabulary; no BC6
/// event type is shared with Cooperative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VmceResultEvent {
    Go,
    PeerReady {
        source: usize,
    },
    PeerLeft {
        source: usize,
    },
    StageIdentity(VmceStageIdentity),
    StageAbsent,
    StageRejected,
    RetryVote {
        source: usize,
    },
    RetryAck {
        source: usize,
    },
    /// The host proved the retry Go reached every guest; the guest now waits
    /// for the host's delivery commit.
    RetryCommit,
    /// The host proved every guest applied the retry commit and reloaded it
    /// with RetryStart; the guest may reload now.
    RetryStart,
    /// A guest's application applied the host's retry delivery commit.
    RetryCommitAck {
        source: usize,
    },
    /// A peer proved the round handoff cannot complete; leave with the reason.
    HandoffFailed(VmceRoundFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VmceRoundStep {
    None,
    /// The host committed the result; load its ending.
    Assign {
        ending: VmceRoundEnding,
    },
    Next,
    Retry,
    CampaignEnd,
    /// Leave the round and return to the shell. `true` carries a visible
    /// handoff failure reason from [`Self::failure`].
    Exit(bool),
    /// A WOL-style room continues without another loading round.
    ContinueInRoom,
}

impl VmceRoundState {
    pub(super) fn new(next_stage: Option<VmceNextStage>) -> Self {
        Self {
            next_stage,
            page_victory: false,
            ending: VmceRoundEnding::Next,
            retry_votes: [false; 4],
            retry_acks: [false; 4],
            retry_go_pending: false,
            retry_commit_acks: [false; 4],
            retry_commit_received: false,
            retry_started: false,
            failure: None,
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

    #[cfg(test)]
    pub(super) fn retry_go_pending(&self) -> bool {
        self.retry_go_pending
    }

    /// Record the guest's one accepted retry Go. A duplicate Go is ignored.
    pub(super) fn mark_retry_go_pending(&mut self) -> bool {
        if self.retry_go_pending {
            return false;
        }
        self.retry_go_pending = true;
        true
    }

    /// Consume the host's retry delivery commit; only a guest that already
    /// accepted the Go may reload.
    pub(super) fn commit_retry_go(&mut self) -> bool {
        if !self.retry_go_pending {
            return false;
        }
        self.retry_go_pending = false;
        true
    }

    pub(super) fn note_retry_ack(&mut self, row: usize) {
        if let Some(ack) = self.retry_acks.get_mut(row) {
            *ack = true;
        }
    }

    pub(super) fn retry_commit_acked(&self, row: usize) -> bool {
        self.retry_commit_acks.get(row).copied().unwrap_or(false)
    }

    pub(super) fn note_retry_commit_ack(&mut self, row: usize) {
        if let Some(ack) = self.retry_commit_acks.get_mut(row) {
            *ack = true;
        }
    }

    #[cfg(test)]
    pub(super) fn retry_commit_received(&self) -> bool {
        self.retry_commit_received
    }

    /// Record the guest's application-level commit. Applied duplicates keep
    /// answering the host's retransmitted commit, so the flag is idempotent.
    pub(super) fn mark_retry_commit_received(&mut self) {
        self.retry_commit_received = true;
    }

    /// Record the guest's one accepted RetryStart. A duplicate returns false
    /// and must not reload a second time.
    pub(super) fn mark_retry_started(&mut self) -> bool {
        if self.retry_started {
            return false;
        }
        self.retry_started = true;
        true
    }

    pub(super) fn failure(&self) -> Option<VmceRoundFailure> {
        self.failure
    }

    /// Stop the round with a visible handoff failure. The first failure stays
    /// authoritative; a later record cannot overwrite the player's reason.
    pub(super) fn note_failure(&mut self, failure: VmceRoundFailure) {
        self.failure.get_or_insert(failure);
    }

    pub(super) fn clear_retry_vote(&mut self, row: usize) {
        if let Some(vote) = self.retry_votes.get_mut(row) {
            *vote = false;
        }
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
        let host = self.host;
        let (previous, connected) = {
            let network = self
                .network
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
            let NetworkPhase::Frozen { startup, connected } = &network.phase else {
                anyhow::bail!("the VMCE result round follows a frozen match");
            };
            (Arc::clone(startup), connected.clone())
        };
        let mut launch = previous.launch_session().clone();
        if let Some(stage) = &next {
            launch.selected_map_file = Some(stage.map_file.clone());
        }
        for human in &mut launch.humans {
            human.slot.start_position = LaunchStartPosition::Auto;
        }
        for ai in &mut launch.opponents {
            ai.start_position = LaunchStartPosition::Auto;
        }
        launch.rebuild_house_roster();
        let mut ready = [true; 8];
        for (index, human) in launch.humans.iter().enumerate() {
            if let Some(row) = human.raw_slot_index
                && connected.contains(&index)
            {
                ready[usize::from(row)] = false;
            }
        }
        let vmce = VmceRoundState::new(next);
        let mut phase = VmceResultPhase {
            ready,
            local_ready: false,
            assigned: false,
            begin: false,
            can_continue,
            rows,
            outcome: None,
            vmce: Some(vmce),
        };
        let network = self.network.as_mut().expect("network visit remains");
        let mut requests = Vec::new();
        if host {
            // VMCE's authored host is ready from the start; only its Continue
            // starts a next stage.
            phase.local_ready = true;
            if let Some(row) = launch
                .humans
                .get(launch.local_player_index)
                .and_then(|human| human.raw_slot_index)
            {
                phase.ready[usize::from(row)] = true;
            }
            requests.push(NetworkLoadingRequest::VmceResultReady);
        }
        if let Some(stage) = phase.vmce.as_ref().and_then(|vmce| vmce.stage_identity()) {
            requests.push(NetworkLoadingRequest::VmceResultStage(stage));
        } else {
            requests.push(NetworkLoadingRequest::VmceResultStageAbsent);
        }
        network.phase = NetworkPhase::VmceResult {
            previous,
            connected,
            launch,
            round: phase,
        };
        network.loading_requests.clear();
        network.loading_requests.extend(requests);
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
        let VmceResultParts { round, .. } = vmce_result_parts(self)?;
        round
            .vmce
            .as_mut()
            .expect("VMCE entry initializes VMCE state")
            .set_ending(ending);
        Ok(())
    }

    pub(crate) fn vmce_round_next_stage_map(&self) -> Option<&str> {
        Some(
            self.vmce_result_phase()?
                .vmce
                .as_ref()?
                .next_stage()?
                .map_file
                .as_str(),
        )
    }

    /// The next-stage unlock this result round has earned: the resolved local
    /// next stage and the round nonce, present only once every online
    /// participant confirmed both the terminal result and this peer's stage
    /// identity. `None` for a retry or a campaign end, which have no next
    /// stage, and before agreement.
    pub(crate) fn vmce_agreed_unlock(&self) -> Option<(&str, [u8; 16])> {
        let nonce = self.vmce_result_nonce()?;
        let network = self.network.as_ref()?;
        let NetworkPhase::VmceResult {
            connected,
            launch,
            round,
            ..
        } = &network.phase
        else {
            return None;
        };
        if !result_agreed(connected, launch, round) {
            return None;
        }
        // Only a next-stage continuation unlocks; a retry or campaign end
        // carries no next stage even if a future caller attaches one.
        if round
            .vmce
            .as_ref()
            .is_none_or(|vmce| vmce.ending() != VmceRoundEnding::Next)
        {
            return None;
        }
        Some((self.vmce_round_next_stage_map()?, nonce))
    }

    /// Reopen the existing VMCE roster on the resolved next map. Transport
    /// handoff must complete before this is exposed as an editable room.
    pub(crate) fn enter_vmce_intermission_setup(&mut self) -> anyhow::Result<VmceNextStage> {
        let network = self
            .network
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
        let NetworkPhase::VmceResult {
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
    let mut participants = 0;
    let all = connected.iter().all(|source| {
        let Some(human) = launch.humans.get(*source) else {
            return true;
        };
        if human.is_observer() {
            return true;
        }
        let Some(slot) = human.raw_slot_index.filter(|slot| *slot < 4) else {
            return true;
        };
        participants += 1;
        seen[usize::from(slot)]
    });
    participants > 0 && all
}

impl MatchSession {
    pub(crate) fn vmce_result_phase(&self) -> Option<&VmceResultPhase> {
        match &self.network.as_ref()?.phase {
            NetworkPhase::VmceResult { round, .. } => Some(round),
            _ => None,
        }
    }

    pub(crate) fn vmce_result_agreement_initialized(&self) -> bool {
        self.vmce_result_phase()
            .and_then(|round| round.vmce.as_ref())
            .is_some_and(VmceRoundState::result_agreement_initialized)
    }

    pub(crate) fn vmce_result_nonce(&self) -> Option<[u8; 16]> {
        self.vmce_result_phase()?.vmce.as_ref()?.result_nonce()
    }

    pub(crate) fn initialize_vmce_result_agreement(
        &mut self,
        entries: [u8; 4],
        nonce: Option<[u8; 16]>,
    ) -> anyhow::Result<()> {
        let host = self.host;
        let VmceResultParts { launch, round, .. } = vmce_result_parts(self)?;
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
    ) -> Option<crate::net::lan_messages::vmce::VmceResultRecord> {
        let NetworkPhase::VmceResult {
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
        Some(crate::net::lan_messages::vmce::VmceResultRecord {
            nonce,
            entries: agreement.entries,
            confirmed,
            stage_digest: vmce.result_stage_digest(nonce),
        })
    }

    pub(crate) fn vmce_result_record_queued(&mut self, confirmed: bool) -> anyhow::Result<()> {
        let VmceResultParts { launch, round, .. } = vmce_result_parts(self)?;
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
        record: crate::net::lan_messages::vmce::VmceResultRecord,
    ) -> anyhow::Result<()> {
        let host = self.host;
        let VmceResultParts {
            connected,
            launch,
            round,
            ..
        } = vmce_result_parts(self)?;
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

    pub(crate) fn vmce_result_take_deferred_go(&mut self) -> anyhow::Result<Option<VmceRoundStep>> {
        let VmceResultParts {
            connected,
            launch,
            round,
            ..
        } = vmce_result_parts(self)?;
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
    round: &VmceResultPhase,
) -> bool {
    round.vmce.as_ref().is_some_and(|vmce| {
        vmce.result_agreement.as_ref().is_some_and(|agreement| {
            agreement.confirmation_sent
                && vmce_all_seen(connected, launch, &agreement.proposed)
                && vmce_all_seen(connected, launch, &agreement.confirmed)
        })
    })
}

pub(super) fn result_step(round: &VmceResultPhase) -> VmceRoundStep {
    round
        .vmce
        .as_ref()
        .expect("VMCE step belongs to a VMCE round")
        .result_step()
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
    round: &VmceResultPhase,
) -> bool {
    let Some(vmce) = round.vmce.as_ref() else {
        return false;
    };
    let agreed = result_agreed(connected, launch, round);
    if round.outcome.is_some() || (host && !agreed) {
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
    round: &mut VmceResultPhase,
) -> anyhow::Result<VmceRoundStep> {
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
        requests.push(NetworkLoadingRequest::VmceResultLeave);
        return Ok(VmceRoundStep::Exit(false));
    }
    if host && round.rows == 1 {
        round.outcome = Some(true);
        return Ok(result_step(round));
    }
    if ending == VmceRoundEnding::Retry {
        if host {
            return Ok(VmceRoundStep::Assign { ending });
        }
        return Ok(VmceRoundStep::None);
    }
    if online {
        round.outcome = Some(false);
        return Ok(VmceRoundStep::ContinueInRoom);
    }
    if host {
        anyhow::ensure!(
            round.all_ready() && !round.assigned,
            "the host Continue waits for every guest"
        );
        log::debug!("VMCE round: host Continue pressed; starting the next stage");
        return Ok(VmceRoundStep::Assign { ending });
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
    requests.push(NetworkLoadingRequest::VmceResultReady);
    Ok(VmceRoundStep::None)
}

pub(super) fn retry_state(session: &MatchSession) -> Option<(bool, bool, bool)> {
    let NetworkPhase::VmceResult {
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

fn retry_vote(session: &mut MatchSession) -> anyhow::Result<VmceRoundStep> {
    anyhow::ensure!(!session.host(), "the host does not cast a retry vote");
    let VmceResultParts {
        requests,
        launch,
        round,
        ..
    } = vmce_result_parts(session)?;
    let vmce = round
        .vmce
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("not a VMCE round"))?;
    anyhow::ensure!(
        vmce.ending() == VmceRoundEnding::Retry,
        "not the VMCE retry page"
    );
    if round.outcome.is_some() {
        return Ok(VmceRoundStep::None);
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
        return Ok(VmceRoundStep::None);
    }
    vmce.note_retry_vote(row);
    requests.push(NetworkLoadingRequest::VmceResultRetryVote);
    Ok(VmceRoundStep::None)
}

fn retry_start(session: &mut MatchSession) -> anyhow::Result<VmceRoundStep> {
    anyhow::ensure!(session.host(), "only the host starts the retry");
    {
        let VmceResultParts {
            connected,
            launch,
            round,
            ..
        } = vmce_result_parts(session)?;
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
    Ok(VmceRoundStep::Assign {
        ending: VmceRoundEnding::Retry,
    })
}

fn set_retry_votes_for_diagnostics(
    session: &mut MatchSession,
    votes: [bool; 4],
) -> anyhow::Result<()> {
    let VmceResultParts { round, .. } = vmce_result_parts(session)?;
    let vmce = round
        .vmce
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("not a VMCE round"))?;
    vmce.retry_votes = votes;
    Ok(())
}

impl MatchSession {
    /// Freeze the result phase into the next network startup. A win reloads the
    /// resolved next stage; a retry reloads the same frozen stage with the same
    /// seed and role assignments. VMCE owns both choices.
    pub(crate) fn freeze_vmce_result(
        &mut self,
        max_ahead_frames: i32,
        latency_fudge: i32,
        saved: Option<crate::sim::campaign_carry_over::CampaignCarryOver>,
    ) -> anyhow::Result<crate::app::match_bootstrap::LoadingStartup> {
        let host = self.host;
        let (previous, launch, connected, round) = {
            let network = self
                .network
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
            let NetworkPhase::VmceResult {
                previous,
                connected,
                launch,
                round,
            } = &network.phase
            else {
                anyhow::bail!("no VMCE result round to continue");
            };
            (
                Arc::clone(previous),
                launch.clone(),
                connected.clone(),
                round.clone(),
            )
        };
        anyhow::ensure!(
            round.outcome == Some(true),
            "the VMCE result round did not continue"
        );
        super::admit_network_start_roster(&launch)?;
        let freeze = prepare_freeze(&previous, &launch, &round)?;
        let mode = previous.mode();
        let carry_over = saved
            .or_else(|| previous.carry_over().copied())
            .unwrap_or(crate::sim::campaign_carry_over::CampaignCarryOver::CLEARED);
        let mut prepared = crate::app::match_bootstrap::PreparedNetworkStartup::new(
            launch,
            freeze.seed,
            mode,
            host,
            max_ahead_frames,
            super::WolStartTerms::default(),
        )
        .with_carry_over(carry_over);
        prepared = freeze.apply_startup(prepared);
        let startup = Arc::new(prepared);
        let network = self
            .network
            .as_mut()
            .expect("network visit remains while freezing the VMCE result");
        freeze.apply_network_identity(network);
        network.max_ahead_frames = Some(max_ahead_frames);
        network.wol_start = super::WolStartTerms::default();
        network.phase = NetworkPhase::Frozen {
            startup: Arc::clone(&startup),
            connected,
        };
        self.latency_fudge = latency_fudge;
        Ok(crate::app::match_bootstrap::LoadingStartup::Network(
            startup,
        ))
    }

    pub(crate) fn vmce_result_ending(&self) -> Option<VmceRoundEnding> {
        self.vmce_result_phase()?.vmce_ending()
    }

    /// Queue the host's Go for the accepted result. The LAN owner changes the
    /// transport scope for the intermission; the control carries the nonce the
    /// peers agreed on.
    pub(crate) fn vmce_result_queue_go(&mut self) -> anyhow::Result<()> {
        let VmceResultParts {
            requests, round, ..
        } = vmce_result_parts(self)?;
        let nonce = round
            .vmce
            .as_ref()
            .and_then(VmceRoundState::result_nonce)
            .ok_or_else(|| anyhow::anyhow!("VMCE result has no agreed nonce"))?;
        requests.push(NetworkLoadingRequest::VmceResultGo { nonce });
        Ok(())
    }

    pub(crate) fn vmce_result_continue(&mut self) -> anyhow::Result<VmceRoundStep> {
        let host = self.host;
        let online = self
            .network
            .as_ref()
            .is_some_and(|network| network.return_context == super::NetworkReturnContext::Wol);
        let VmceResultParts {
            requests,
            connected,
            launch,
            round,
        } = vmce_result_parts(self)?;
        continue_round(host, online, requests, connected, launch, round)
    }

    /// Every connected guest consumed the host's retry Go.
    pub(crate) fn vmce_result_retry_acks_complete(&self) -> bool {
        let Some(NetworkPhase::VmceResult {
            connected,
            launch,
            round,
            ..
        }) = self.network.as_ref().map(|network| &network.phase)
        else {
            return false;
        };
        let Some(vmce) = round.vmce.as_ref() else {
            return false;
        };
        connected.iter().all(|index| {
            let Some(human) = launch.humans.get(*index) else {
                return true;
            };
            if human.is_observer() {
                return true;
            }
            match human.raw_slot_index {
                Some(0) | None => true,
                Some(row) if row < 4 => vmce.retry_acks[usize::from(row)],
                Some(_) => true,
            }
        })
    }

    /// This guest accepted the retry Go and still waits for the host commit.
    #[cfg(test)]
    pub(crate) fn vmce_result_retry_go_pending(&self) -> bool {
        self.vmce_result_phase()
            .and_then(|round| round.vmce.as_ref())
            .is_some_and(VmceRoundState::retry_go_pending)
    }

    /// Every connected guest's application applied the host's retry commit.
    pub(crate) fn vmce_result_retry_commit_acks_complete(&self) -> bool {
        let Some(NetworkPhase::VmceResult {
            connected,
            launch,
            round,
            ..
        }) = self.network.as_ref().map(|network| &network.phase)
        else {
            return false;
        };
        let Some(vmce) = round.vmce.as_ref() else {
            return false;
        };
        connected.iter().all(|index| {
            let Some(human) = launch.humans.get(*index) else {
                return true;
            };
            if human.is_observer() {
                return true;
            }
            match human.raw_slot_index {
                Some(0) | None => true,
                Some(row) if row < 4 => vmce.retry_commit_acked(usize::from(row)),
                Some(_) => true,
            }
        })
    }

    /// This guest applied the retry commit and waits for the host's start.
    #[cfg(test)]
    pub(crate) fn vmce_result_retry_commit_received(&self) -> bool {
        self.vmce_result_phase()
            .and_then(|round| round.vmce.as_ref())
            .is_some_and(VmceRoundState::retry_commit_received)
    }

    /// The bounded handoff failure this round must report when it exits.
    pub(crate) fn vmce_result_failure(&self) -> Option<VmceRoundFailure> {
        self.vmce_result_phase()
            .and_then(|round| round.vmce.as_ref())
            .and_then(VmceRoundState::failure)
    }

    /// Stop the round locally with a visible handoff failure: the page closes,
    /// the departure owner notifies peers, and nothing may reload.
    pub(crate) fn vmce_result_abort(&mut self, failure: VmceRoundFailure) -> anyhow::Result<()> {
        let VmceResultParts {
            round, requests, ..
        } = vmce_result_parts(self)?;
        round.outcome = Some(false);
        // Requests from the abandoned round must not be sent after its failure
        // notice, for example when the result owner's FIFO was backpressured.
        requests.clear();
        if let Some(vmce) = round.vmce.as_mut() {
            vmce.note_failure(failure);
            vmce.retry_go_pending = false;
        }
        Ok(())
    }

    pub(crate) fn vmce_result_continue_enabled(&self) -> bool {
        let NetworkPhase::VmceResult {
            connected,
            launch,
            round,
            ..
        } = &self
            .network
            .as_ref()
            .map(|network| &network.phase)
            .unwrap_or(&NetworkPhase::Lobby)
        else {
            return false;
        };
        continue_enabled(self.host, connected, launch, round)
    }

    pub(crate) fn vmce_result_slot_ready(&self, index: usize) -> bool {
        let Some(NetworkPhase::VmceResult { launch, round, .. }) =
            self.network.as_ref().map(|network| &network.phase)
        else {
            return false;
        };
        launch
            .humans
            .get(index)
            .and_then(|human| human.raw_slot_index)
            .is_some_and(|row| round.ready[usize::from(row)])
    }

    pub(crate) fn vmce_result_can_continue(&self) -> bool {
        self.vmce_result_phase()
            .is_some_and(VmceResultPhase::can_continue)
    }

    /// The host's readiness rows as `(ready, total, local)`.
    pub(crate) fn vmce_result_ready_progress(&self) -> Option<(usize, usize, bool)> {
        let NetworkPhase::VmceResult {
            connected,
            launch,
            round,
            ..
        } = self.network.as_ref().map(|network| &network.phase)?
        else {
            return None;
        };
        let mut ready = 0;
        let mut total = 0;
        for (index, human) in launch.humans.iter().enumerate() {
            if human.is_observer() || !connected.contains(&index) {
                continue;
            }
            let Some(row) = human.raw_slot_index else {
                continue;
            };
            total += 1;
            ready += usize::from(round.ready[usize::from(row)]);
        }
        Some((ready, total, round.local_ready))
    }

    pub(crate) fn vmce_result_retry_state(&self) -> Option<(bool, bool, bool)> {
        retry_state(self)
    }

    pub(crate) fn set_vmce_result_page_victory(&mut self, victory: bool) {
        if let Some(round) = self
            .network
            .as_mut()
            .and_then(|network| match &mut network.phase {
                NetworkPhase::VmceResult { round, .. } => Some(round),
                _ => None,
            })
        {
            if let Some(vmce) = round.vmce.as_mut() {
                vmce.set_page_victory(victory);
            }
        }
    }

    /// Retry-page projection used by the app and capture diagnostics.
    pub(crate) fn vmce_result_retry_vote(&mut self) -> anyhow::Result<VmceRoundStep> {
        retry_vote(self)
    }

    /// Host retry start after the child round confirms all guest votes.
    pub(crate) fn vmce_result_retry_start(&mut self) -> anyhow::Result<VmceRoundStep> {
        retry_start(self)
    }

    /// Set fixture votes for the synthetic defeat-page capture.
    pub(crate) fn vmce_result_set_retry_votes_for_diagnostics(
        &mut self,
        votes: [bool; 4],
    ) -> anyhow::Result<()> {
        set_retry_votes_for_diagnostics(self, votes)
    }

    /// Commit the agreed result on the host side: the next loading step may
    /// freeze this phase.
    pub(crate) fn vmce_result_assigned(&mut self) -> anyhow::Result<()> {
        let network = self
            .network
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
        let NetworkPhase::VmceResult {
            connected,
            launch,
            round,
            ..
        } = &network.phase
        else {
            anyhow::bail!("no VMCE result round");
        };
        anyhow::ensure!(
            result_agreed(connected, launch, round),
            "VMCE result assignment is not agreed"
        );
        let NetworkPhase::VmceResult { round, .. } = &mut network.phase else {
            anyhow::bail!("no VMCE result round");
        };
        round.outcome = Some(true);
        round.assigned = true;
        Ok(())
    }

    /// Leave the VMCE result phase without a network round.
    pub(crate) fn quit_vmce_result(&mut self) -> anyhow::Result<()> {
        let network = self
            .network
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no network visit"))?;
        let NetworkPhase::VmceResult {
            previous,
            connected,
            ..
        } = &network.phase
        else {
            anyhow::bail!("no VMCE result round");
        };
        let previous = Arc::clone(previous);
        let connected = connected.clone();
        network.phase = NetworkPhase::Frozen {
            startup: previous,
            connected,
        };
        network.loading_requests.clear();
        Ok(())
    }

    /// Apply one decoded VMCE result event and return the app step it produced.
    pub(crate) fn vmce_result_event(
        &mut self,
        event: VmceResultEvent,
    ) -> anyhow::Result<VmceRoundStep> {
        let host = self.host;
        let VmceResultParts {
            requests,
            launch,
            round,
            ..
        } = vmce_result_parts(self)?;
        let row_of = |source: usize| {
            launch
                .humans
                .get(source)
                .and_then(|human| human.raw_slot_index)
                .map(usize::from)
                .filter(|row| *row < 4)
        };
        match event {
            VmceResultEvent::Go => {
                round.begin = true;
                round.assigned = true;
                if host {
                    return Ok(VmceRoundStep::None);
                }
                // The host's Go is the guest's accepted result: Next opens the
                // agreed intermission, Retry waits for the host's delivery
                // commit, CampaignEnd ends the campaign on both peers.
                round.outcome = Some(true);
                let vmce = round
                    .vmce
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("not a VMCE round"))?;
                let ending = vmce.ending();
                if ending == VmceRoundEnding::Retry {
                    if vmce.mark_retry_go_pending() {
                        requests.push(NetworkLoadingRequest::VmceResultRetryAck);
                    }
                    return Ok(VmceRoundStep::None);
                }
                Ok(vmce.result_step())
            }
            VmceResultEvent::PeerReady { source } => {
                if let Some(row) = row_of(source) {
                    round.ready[row] = true;
                    log::debug!("VMCE result: peer source={source} row={row} is ready");
                }
                Ok(VmceRoundStep::None)
            }
            VmceResultEvent::PeerLeft { source } => {
                Ok(peer_left(round, row_of(source), requests).unwrap_or(VmceRoundStep::None))
            }
            VmceResultEvent::StageIdentity(advertised) => {
                Ok(stage_event(round, requests, Some(&advertised)))
            }
            VmceResultEvent::StageAbsent => Ok(stage_event(round, requests, None)),
            VmceResultEvent::StageRejected => Ok(stage_rejected(round, requests)),
            VmceResultEvent::RetryVote { source } => {
                retry_vote_received(round, row_of(source));
                Ok(VmceRoundStep::None)
            }
            VmceResultEvent::RetryAck { source } => {
                if let (Some(row), Some(vmce)) = (row_of(source), round.vmce.as_mut()) {
                    vmce.note_retry_ack(row);
                }
                Ok(VmceRoundStep::None)
            }
            VmceResultEvent::RetryCommit => {
                // The guest applies the delivery commit now and proves it with
                // an application ACK; it still stays on the page until the
                // host's RetryStart. A retransmitted commit re-answers the
                // host, which repairs a lost application ACK.
                if !host
                    && round.outcome.is_some()
                    && let Some(vmce) = round.vmce.as_mut()
                    && vmce.ending() == VmceRoundEnding::Retry
                {
                    let _ = vmce.commit_retry_go();
                    vmce.mark_retry_commit_received();
                    requests.push(NetworkLoadingRequest::VmceResultRetryCommitAck);
                }
                Ok(VmceRoundStep::None)
            }
            VmceResultEvent::RetryStart => {
                if !host
                    && round.outcome.is_some()
                    && let Some(vmce) = round.vmce.as_mut()
                    && vmce.ending() == VmceRoundEnding::Retry
                    && vmce.mark_retry_started()
                {
                    return Ok(VmceRoundStep::Retry);
                }
                Ok(VmceRoundStep::None)
            }
            VmceResultEvent::RetryCommitAck { source } => {
                if host && let (Some(row), Some(vmce)) = (row_of(source), round.vmce.as_mut()) {
                    vmce.note_retry_commit_ack(row);
                }
                Ok(VmceRoundStep::None)
            }
            VmceResultEvent::HandoffFailed(failure) => {
                round.outcome = Some(false);
                requests.clear();
                if let Some(vmce) = round.vmce.as_mut() {
                    vmce.note_failure(failure);
                    vmce.retry_go_pending = false;
                }
                requests.push(NetworkLoadingRequest::VmceResultLeave);
                Ok(VmceRoundStep::Exit(true))
            }
        }
    }
}

pub(super) fn stage_event(
    round: &mut VmceResultPhase,
    requests: &mut Vec<NetworkLoadingRequest>,
    advertised: Option<&VmceStageIdentity>,
) -> VmceRoundStep {
    let matches = round.vmce.as_ref().is_some_and(|vmce| match advertised {
        Some(stage) => vmce.stage_identity_matches(stage),
        None => !vmce.has_stage(),
    });
    if matches {
        return VmceRoundStep::None;
    }
    log::warn!(
        "VMCE next-stage identity mismatch; returning both peers to the lobby instead of loading"
    );
    round.outcome = Some(false);
    requests.push(NetworkLoadingRequest::VmceResultStageRejected);
    VmceRoundStep::Exit(false)
}

pub(super) fn stage_rejected(
    round: &mut VmceResultPhase,
    requests: &mut Vec<NetworkLoadingRequest>,
) -> VmceRoundStep {
    round.outcome = Some(false);
    requests.push(NetworkLoadingRequest::VmceResultLeave);
    VmceRoundStep::Exit(false)
}

pub(super) fn retry_vote_received(round: &mut VmceResultPhase, row: Option<usize>) {
    if let (Some(vmce), Some(row)) = (round.vmce.as_mut(), row.filter(|row| *row < 4)) {
        vmce.note_retry_vote(row);
    }
}

pub(super) fn peer_left(
    round: &mut VmceResultPhase,
    row: Option<usize>,
    requests: &mut Vec<NetworkLoadingRequest>,
) -> Option<VmceRoundStep> {
    let vmce = round.vmce.as_mut()?;
    if let Some(row) = row.filter(|row| *row < 4) {
        vmce.clear_retry_vote(row);
    }
    round.outcome = Some(false);
    requests.push(NetworkLoadingRequest::VmceResultLeave);
    Some(VmceRoundStep::Exit(false))
}

pub(super) struct FreezeData {
    pub(super) seed: u32,
    retry_nonce: Option<[u8; 16]>,
    previous_houses: Option<crate::skirmish_launch::VmceHouseAssignments>,
    next_houses: Option<crate::skirmish_launch::VmceHouseAssignments>,
    next_stage: Option<VmceNextStage>,
    preserved_map_identity: Option<(
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
        } else if let Some((config, source, digest)) = self.preserved_map_identity.as_ref() {
            network.vmce_map_config = config.clone();
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
        } else if let Some((config, source, digest)) = self.preserved_map_identity.clone() {
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
    round: &VmceResultPhase,
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
    // Reuse the admitted map identity only when the next launch still names
    // that selected map. A changed stock Cooperative map must resolve its own
    // source through the normal loader path.
    let same_previous_map = previous
        .launch_session()
        .selected_map_file
        .as_deref()
        .zip(launch.selected_map_file.as_deref())
        .is_some_and(|(previous, next)| previous.eq_ignore_ascii_case(next));
    let preserved_map_identity = next_stage.is_none().then(|| {
        if same_previous_map {
            (
                previous.vmce_map_config().cloned(),
                previous.local_map_source().cloned(),
                previous.effective_map_digest().map(str::to_owned),
            )
        } else {
            (None, None, None)
        }
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
        preserved_map_identity,
    })
}

#[cfg(test)]
#[path = "vmce_result/tests.rs"]
mod tests;
