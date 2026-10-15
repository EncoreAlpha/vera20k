//! A Techno's playfield membership (`TechnoClass+0x3D5`): the record and the
//! writers that compute it from the map's playfield.

use super::{Simulation, UninitContext};
use crate::map::resolved_terrain::ResolvedTerrainGrid;

/// `TechnoClass+0x3D5`, the stored fact that the object is inside the map's
/// playfield. Consumers read it rather than a fresh bounds query, which would
/// erase the native movement hysteresis. The constructor clears it
/// (`0x006F2F5B`).
///
/// Writers:
/// - [`Self::enter`]: `BuildingClass::ExitObject_Main`'s prelude
///   (`0x00443C81`), whatever the exit's outcome; Fly's playfield latch
///   (`0x004CD5F0`); ordinary cell movement (`0x006F511A..0x006F5139`, the
///   Teleport warp's PerCell(2) included), which only promotes.
/// - [`Self::store`]: the exact mode-one result of `TechnoClass::Unlimbo`
///   (`0x006F6CFE`) and of `MapClass::Set_Clipped_LocalSize @ 0x00567230`
///   (`0x0056730E`), which may demote as well as promote.
/// - [`Self::leave`]: the Chronosphere warp's arrival outside the playfield
///   (`0x00719A99`, `movement::teleport_chrono`).
///
/// RESIDUAL: `FootClass::AI`'s per-frame promotion (`0x004DA63B..0x004DA670`:
/// a Foot that is not a member, of a type that is not `BalloonHover=`, whose
/// cell is in the playfield) is not ported; VERA promotes on cell movement
/// alone. Trigger: a Foot that entered the playfield from outside it
/// (reinforcements and other off-map entries). Effect: until its cell
/// movement promotes it, membership consumers can still see it outside.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct PlayfieldMembership(bool);

impl PlayfieldMembership {
    /// The stored membership.
    pub(crate) const fn get(self) -> bool {
        self.0
    }

    /// Promote to a member: the exit prelude, Fly's latch and cell movement.
    pub(crate) fn enter(&mut self) {
        self.0 = true;
    }

    /// Store an exact recomputation: Unlimbo and `Set_Clipped_LocalSize`.
    pub(crate) fn store(&mut self, member: bool) {
        self.0 = member;
    }

    /// The Chronosphere arrival's clear outside the playfield.
    pub(crate) fn leave(&mut self) {
        self.0 = false;
    }
}

impl Simulation {
    /// Exact mode-one query for the stored TechnoClass+0x3D5 writer family.
    /// Absence means there is no live MapClass authority, not rectangular or
    /// permissive fallback authority.
    pub(super) fn entity_playfield_membership_mode_one(
        &self,
        stable_id: u64,
        terrain: Option<&ResolvedTerrainGrid>,
    ) -> Option<bool> {
        let bounds = self.playfield_bounds?;
        let entity = self.substrate.entities.get(stable_id)?;
        Some(crate::sim::cell_rect::cell_is_in_playfield_height_aware(
            (
                i32::from(entity.position.rx()),
                i32::from(entity.position.ry()),
            ),
            Some(bounds),
            terrain,
        ))
    }

    /// Unlimbo's exact establishment writer (`TechnoClass::Unlimbo @
    /// 0x006F6CFE`).
    pub(super) fn establish_entity_playfield_membership_on_unlimbo(
        &mut self,
        stable_id: u64,
        context: UninitContext<'_>,
    ) {
        let Some(member) = self.entity_playfield_membership_mode_one(
            stable_id,
            context.terrain().or(self.resolved_terrain.as_ref()),
        ) else {
            return;
        };
        if let Some(entity) = self.substrate.entities.get_mut(stable_id) {
            entity.in_playfield.store(member);
        }
    }

    /// Ordinary per-cell movement writer (`0x006F511A..0x006F5139`): only
    /// promote 0 -> 1. A unit that walks back outside retains membership until
    /// an exact writer (teleport or Set_Clipped_LocalSize) clears it.
    pub(crate) fn promote_entity_playfield_membership_after_move(&mut self, stable_id: u64) {
        if self
            .substrate
            .entities
            .get(stable_id)
            .is_none_or(|entity| entity.in_playfield())
        {
            return;
        }
        if self.entity_playfield_membership_mode_one(stable_id, self.resolved_terrain.as_ref())
            == Some(true)
            && let Some(entity) = self.substrate.entities.get_mut(stable_id)
        {
            entity.in_playfield.enter();
        }
    }
}
