//! VMCE's campaign session state helpers on `ScenarioSession`.
//!
//! The session struct keeps the authoritative fields; this module owns the
//! VMCE-only install, query and hash-fold logic so the shared session file
//! carries only the gated fields and the seam used by stock code.

use std::hash::{Hash, Hasher};

use super::ScenarioSession;
use crate::sim::house_state::HouseOutcomeKind;
use crate::sim::intern::InternedId;

impl ScenarioSession {
    pub(crate) fn install_network_house_grants(
        &mut self,
        source_houses: Vec<u8>,
        grants: Vec<Vec<u8>>,
        local_source: Option<i8>,
    ) {
        debug_assert_eq!(source_houses.len(), grants.len());
        self.shared_house_sources = source_houses;
        self.network_house_grants = grants;
        self.local_shared_house_source = local_source;
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn install_network_house_grants_for_test(
        &mut self,
        source_houses: Vec<u8>,
        grants: Vec<Vec<u8>>,
        local_source: Option<i8>,
    ) {
        self.install_network_house_grants(source_houses, grants, local_source);
    }

    /// Each VMCE network source's primary House index, in source order.
    /// Sources with no grant row (an observer or a non-participant seat) are
    /// not players and are omitted: mission viewpoints read only this list.
    pub(crate) fn campaign_source_houses(&self) -> impl Iterator<Item = u8> + '_ {
        self.shared_house_sources
            .iter()
            .zip(&self.network_house_grants)
            .filter(|(_, grants)| !grants.is_empty())
            .map(|(&house, _)| house)
    }

    /// Install the VMCE frozen lobby difficulty row. The caller has
    /// already validated the 0..=2 range; clamp here keeps a corrupted frozen
    /// payload from indexing past TriggerType's three flags.
    pub(crate) fn install_vmce_trigger_difficulty(&mut self, difficulty: i32) {
        debug_assert!((0..=2).contains(&difficulty), "lobby row was validated");
        self.vmce_trigger_difficulty = Some(difficulty.clamp(0, 2));
    }

    /// VMCE starts an authored campaign map under a nonzero network game
    /// mode. The map's campaign defeat and economy rules are not the stock
    /// skirmish defaults: the role defeat gate must count all objects (not
    /// `ShortGame` buildings only), and random lobby crates are a multiplayer
    /// system the campaign never runs.
    pub(crate) fn install_vmce_series_options(&mut self) {
        self.game_options.short_game = false;
        self.game_options.crates = false;
    }

    pub(crate) fn record_vmce_outcome_candidate(
        &mut self,
        owner: InternedId,
        kind: crate::sim::house_state::HouseOutcomeKind,
    ) {
        if self.vmce_outcomes.is_some() {
            return;
        }
        if let Some((_, previous)) = self
            .vmce_outcome_candidates
            .iter_mut()
            .find(|(id, _)| *id == owner)
        {
            *previous = kind;
        } else {
            self.vmce_outcome_candidates.push((owner, kind));
        }
    }

    pub(crate) fn vmce_outcome_candidate(
        &self,
        owner: InternedId,
    ) -> Option<crate::sim::house_state::HouseOutcomeKind> {
        self.vmce_outcome_candidates
            .iter()
            .find_map(|(id, kind)| (*id == owner).then_some(*kind))
    }

    pub(crate) fn latch_vmce_outcomes(
        &mut self,
        outcomes: Vec<(InternedId, crate::sim::house_state::HouseOutcomeKind)>,
    ) {
        if self.vmce_outcomes.is_none() {
            self.vmce_outcomes = Some(outcomes);
        }
    }

    pub fn vmce_outcome(
        &self,
        owner: InternedId,
    ) -> Option<crate::sim::house_state::HouseOutcomeKind> {
        self.vmce_outcomes
            .as_ref()?
            .iter()
            .find_map(|(id, kind)| (*id == owner).then_some(*kind))
    }

    pub fn vmce_outcomes(
        &self,
    ) -> Option<&[(InternedId, crate::sim::house_state::HouseOutcomeKind)]> {
        self.vmce_outcomes.as_deref()
    }

    pub(crate) fn vmce_outcomes_valid(&self, viewpoints: &[InternedId]) -> bool {
        let valid_candidates =
            self.vmce_outcome_candidates
                .iter()
                .enumerate()
                .all(|(i, (owner, _))| {
                    viewpoints.contains(owner)
                        && !self.vmce_outcome_candidates[..i]
                            .iter()
                            .any(|(previous, _)| previous == owner)
                });
        let valid_final = self.vmce_outcomes.as_ref().is_none_or(|outcomes| {
            outcomes.len() == viewpoints.len()
                && outcomes
                    .iter()
                    .zip(viewpoints)
                    .all(|((owner, _), expected)| owner == expected)
        });
        let vmce = self
            .multiplayer_mode_class
            .is_some_and(|class| class.is_vmce());
        valid_candidates
            && valid_final
            && (vmce || (self.vmce_outcome_candidates.is_empty() && self.vmce_outcomes.is_none()))
    }
}

impl ScenarioSession {
    pub(crate) fn fold_vmce_house_grants(&self, hasher: &mut impl Hasher) {
        if !self.network_house_grants.is_empty() {
            b"vera-campaign-house-grants-v1".hash(hasher);
            self.shared_house_sources.hash(hasher);
            self.network_house_grants.hash(hasher);
        }
    }

    pub(crate) fn fold_vmce_trigger_difficulty(&self, hasher: &mut impl Hasher) {
        if let Some(difficulty) = self.vmce_trigger_difficulty {
            b"scenario-vera-campaign-trigger-difficulty-v1".hash(hasher);
            difficulty.hash(hasher);
        }
    }

    pub(crate) fn fold_vmce_outcomes(&self, hasher: &mut impl Hasher) {
        if !self.vmce_outcome_candidates.is_empty() || self.vmce_outcomes.is_some() {
            b"vmce-outcomes-v1".hash(hasher);
            self.vmce_outcome_candidates.hash(hasher);
            self.vmce_outcomes.hash(hasher);
        }
    }
}
