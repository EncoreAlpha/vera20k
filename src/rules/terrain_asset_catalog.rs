//! GPU-independent native TerrainType/terrain-spawner SHP metadata.
//!
//! TerrainTypeClass::ReadINI @ 0x0071DEA0 also reads frame 0's raw SHP-header
//! RGB before its per-pass RadarColor override. The same owner binds that
//! derived type value and the raw frame counts used by TerrainClass::AI.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::assets::asset_manager::AssetManager;
use crate::assets::shp_file::ShpFile;
use crate::rules::art_data::{self, ArtRegistry};
use crate::rules::ini_parser::{IniFile, IniSection};
use crate::rules::terrain_object_type::TerrainObjectType;

/// Terrain SHP metadata for native RadarColor reads and animated ore spawners.
///
/// Frame counts remain keyed by terrain-type ID. Frame-0 RGB is applied
/// directly to the existing `TerrainObjectType` owner after its ordered Rules
/// read inputs are replayed; the per-bind image lookup map is discarded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct TerrainAssetCatalog {
    frame_counts: BTreeMap<String, u16>,
}

impl TerrainAssetCatalog {
    /// Bind native TerrainType frame-0 colors and animated spawner frame counts.
    pub fn bind(
        art: &ArtRegistry,
        terrain_types: &mut HashMap<String, TerrainObjectType>,
        rules_ini: &IniFile,
        asset_manager: &AssetManager,
        theater_ext: &str,
        theater_name: &str,
    ) -> Self {
        let mut image_colors = BTreeMap::<String, Option<[u8; 3]>>::new();
        for terrain in terrain_types.values_mut() {
            let mut current = [0; 3];
            for read in &terrain.radar_color_read_passes {
                let image_key = read.image.to_ascii_uppercase();
                let image_color = *image_colors.entry(image_key).or_insert_with(|| {
                    // TerrainTypeClass::ReadINI uses its current +0x1F8
                    // ImageFile directly to form `<Image>.SHP` at 0x71DFA3;
                    // do not follow an ART `Image=` redirect here. Reuse the
                    // existing object theater-candidate policy only.
                    let candidates = art_data::object_shp_candidates(
                        Some(art),
                        &read.image,
                        theater_ext,
                        theater_name,
                    );
                    candidates.iter().find_map(|candidate| {
                        let bytes = asset_manager.get_ref(candidate)?;
                        ShpFile::frame_radar_color_from_bytes(bytes, 0).ok()
                    })
                });
                if let Some(image_color) = image_color {
                    current = image_color;
                }
                if let Some(raw) = read.radar_color.as_deref() {
                    current = IniSection::read_color_rgb_value(raw, current);
                }
            }
            // Replaying from the constructor's zero makes repeated binds
            // deterministic and leaves this as the sole final typed value.
            terrain.radar_color = current;
        }

        let roots: BTreeSet<String> = terrain_types
            .values()
            .filter(|terrain| terrain.spawns_tiberium && terrain.is_animated)
            .filter_map(|terrain| canonical_type_id(&terrain.name))
            .collect();
        let mut catalog = Self::default();

        for name in roots {
            let image_id = art.resolve_overlay_image_id(&name, rules_ini);
            let candidates = art_data::overlay_shp_candidates(
                Some(art),
                &name,
                &image_id,
                theater_ext,
                theater_name,
            );
            let mut found_asset = false;
            let mut invalid_candidates = Vec::new();
            let mut frame_count = None;
            for candidate in &candidates {
                let Some(data) = asset_manager.get_ref(candidate) else {
                    continue;
                };
                found_asset = true;
                match ShpFile::frame_count_from_bytes(data) {
                    Ok(count) => {
                        frame_count = Some(count);
                        break;
                    }
                    Err(error) => invalid_candidates.push(format!("{candidate}: {error}")),
                }
            }

            if let Some(frame_count) = frame_count {
                catalog.frame_counts.insert(name, frame_count);
            } else if found_asset {
                log::warn!(
                    "Authoritative terrain-spawner asset [{name}] has no valid SHP candidate ({invalid_candidates:?}); no validated spawning midpoint is available"
                );
            } else {
                log::warn!(
                    "Authoritative terrain-spawner asset [{name}] is missing; no validated spawning midpoint is available"
                );
            }
        }

        catalog
    }

    /// Literal unsigned SHP header count used by the persisted terrain-spawner
    /// state. Stock TIBTRE assets return 22 and therefore target midpoint 11.
    pub fn frame_count(&self, name: &str) -> Option<u16> {
        canonical_type_id(name).and_then(|name| self.frame_counts.get(&name).copied())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_for_test(&mut self, name: &str, frame_count: u16) {
        if let Some(name) = canonical_type_id(name) {
            self.frame_counts.insert(name, frame_count);
        }
    }
}

fn canonical_type_id(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::ini_parser::IniFile;
    use crate::rules::native_processing::{RulesLayerKind, RulesLayerStack};
    use crate::rules::ruleset::RuleSet;

    #[test]
    fn terrain_radar_color_matches_stock_tree_after_asset_bind() {
        let Some(mut battle) = crate::rules::retail_ini_fixture::retail_battle_rules() else {
            return;
        };
        let Some((_, mut assets)) = crate::rules::retail_ini_fixture::retail_scenario_assets()
        else {
            return;
        };
        let theater = crate::map::theater::load_theater(&mut assets, "TEMPERATE")
            .expect("stock Battle theater");

        battle.rules.bind_terrain_assets(
            &battle.processed_rules,
            &assets,
            theater.extension,
            "TEMPERATE",
        );
        let tree = battle
            .rules
            .terrain_object_type_case_insensitive("TREE01")
            .expect("stock Battle registers TREE01");
        assert_eq!(tree.radar_color, [0, 192, 0]);
    }

    #[test]
    fn terrain_radar_color_reloads_current_image_each_rules_pass() {
        let directory = crate::map::source::test_support::TestDirectory::new("terrain-radar-color");
        directory.write("ROOT.SHP", &test_shp_with_radar_color([10, 20, 30]));
        directory.write("MODE.SHP", &test_shp_with_radar_color([40, 50, 60]));
        let assets = AssetManager::from_loose_root_for_test(directory.path());

        let mut layers = RulesLayerStack::new(IniFile::from_str(
            "[TerrainTypes]\n0=RADAR_TEST\n[RADAR_TEST]\nImage=ROOT\n",
        ));
        layers.push(
            RulesLayerKind::GameMode,
            IniFile::from_str("[RADAR_TEST]\nImage=MODE\nRadarColor=90,91,92\n"),
        );
        layers.push(
            RulesLayerKind::Scenario,
            IniFile::from_str("[RADAR_TEST]\nRadarColor=invalid\n"),
        );
        layers.push(
            RulesLayerKind::Scenario,
            IniFile::from_str("[RADAR_TEST]\nStrength=1\n"),
        );
        let processed = layers.process().expect("production rules passes");
        let mut rules = RuleSet::from_processed_rules(&processed).expect("typed Rules projection");
        rules.bind_terrain_assets(processed.ini(), &assets, "tem", "TEMPERATE");
        assert_eq!(
            rules
                .terrain_object_type_case_insensitive("RADAR_TEST")
                .expect("registered terrain type")
                .radar_color,
            [40, 50, 60],
            "the later absent/invalid RadarColor reads use the successfully loaded current Image frame"
        );

        layers.push(
            RulesLayerKind::Scenario,
            IniFile::from_str("[RADAR_TEST]\nRadarColor=70,80,90\n"),
        );
        layers.push(
            RulesLayerKind::Scenario,
            IniFile::from_str("[RADAR_TEST]\nImage=MISSING\n"),
        );
        let processed = layers.process().expect("extended production rules passes");
        let mut rules = RuleSet::from_processed_rules(&processed).expect("typed Rules projection");
        rules.bind_terrain_assets(processed.ini(), &assets, "tem", "TEMPERATE");
        assert_eq!(
            rules
                .terrain_object_type_case_insensitive("RADAR_TEST")
                .expect("registered terrain type")
                .radar_color,
            [70, 80, 90],
            "a missing Image SHP skips GetColor and preserves the current RadarColor"
        );
    }

    fn test_shp_with_radar_color(color: [u8; 3]) -> Vec<u8> {
        let mut data = vec![0; 8 + 24 + 1];
        data[2..4].copy_from_slice(&1u16.to_le_bytes());
        data[4..6].copy_from_slice(&1u16.to_le_bytes());
        data[6..8].copy_from_slice(&1u16.to_le_bytes());
        data[8 + 4..8 + 6].copy_from_slice(&1u16.to_le_bytes());
        data[8 + 6..8 + 8].copy_from_slice(&1u16.to_le_bytes());
        data[8 + 12..8 + 15].copy_from_slice(&color);
        data[8 + 20..8 + 24].copy_from_slice(&32u32.to_le_bytes());
        data[32] = 1;
        data
    }
}
