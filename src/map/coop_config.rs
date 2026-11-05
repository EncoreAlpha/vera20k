//! Whitelisted external map settings for VERA cooperative multiplayer.

use std::collections::{BTreeMap, BTreeSet};

use crate::rules::ini_parser::IniFile;

/// Normalized external settings admitted for a VERA Coop map.
///
/// Only `[Basic] Player1..Player4` and control fields for Houses already named
/// by the map's `[Houses]` section can be represented. Fields stay private so
/// deserialized values receive the same validation when applied as source INI
/// values do when parsed here.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VeraCoopMapConfig {
    #[serde(default)]
    basic_players: [Option<String>; 4],
    #[serde(default)]
    houses: BTreeMap<String, VeraCoopHouseConfig>,
    /// Campaign INIs replace map-authored cooperative assignments entirely.
    #[serde(default)]
    replace_map_values: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct VeraCoopHouseConfig {
    #[serde(default)]
    player_control: Option<bool>,
    #[serde(default)]
    player_controls: [Option<bool>; 4],
}

impl VeraCoopMapConfig {
    /// Parse raw supplemental INI bytes with empty-value presence retained,
    /// then validate and normalize the allowed fields.
    pub fn from_bytes(map_ini: &IniFile, supplemental: &[u8]) -> anyhow::Result<Self> {
        let supplemental = IniFile::from_bytes_preserving_empty_values(supplemental)
            .map_err(anyhow::Error::from)?;
        Self::from_ini(map_ini, &supplemental)
    }

    /// Validate the complete supplemental document and normalize only its
    /// allowed values through the existing INI readers.
    pub fn from_ini(map_ini: &IniFile, supplemental: &IniFile) -> anyhow::Result<Self> {
        let houses = listed_houses(map_ini);
        validate_supplemental(supplemental, &houses)?;

        let mut config = Self::default();
        if let Some(basic) = supplemental.section("Basic") {
            for (index, player) in config.basic_players.iter_mut().enumerate() {
                let key = format!("Player{}", index + 1);
                if basic.is_present(&key) || basic.has_explicit_empty_value(&key) {
                    *player = Some(basic.read_name(&key, 0x14).unwrap_or_default().to_owned());
                }
            }
        }

        for house_name in houses {
            let Some(section) = supplemental.section(&house_name) else {
                continue;
            };
            let mut values = VeraCoopHouseConfig::default();
            values.player_control = read_control(section, "PlayerControl")?;
            for (index, control) in values.player_controls.iter_mut().enumerate() {
                *control = read_control(section, &format!("Player{}Control", index + 1))?;
            }
            if values.player_control.is_some() || values.player_controls.iter().any(Option::is_some)
            {
                config.houses.insert(house_name, values);
            }
        }
        Ok(config)
    }

    /// Apply the normalized settings to a clone of the authored map. The
    /// roster is checked again because configs may have arrived via serde.
    pub fn apply_to_map(&self, map_ini: &IniFile) -> anyhow::Result<IniFile> {
        let listed = listed_houses(map_ini);
        let mut values = Vec::<(String, String, String)>::new();
        for (index, player) in self.basic_players.iter().enumerate() {
            if let Some(player) = player {
                validate_player_name(player)?;
                anyhow::ensure!(
                    player.is_empty() || listed.contains(player),
                    "external [Basic] Player{} names House {player:?} absent from map [Houses]",
                    index + 1
                );
                values.push((
                    "Basic".to_owned(),
                    format!("Player{}", index + 1),
                    player.clone(),
                ));
            }
        }
        for (house_name, fields) in &self.houses {
            anyhow::ensure!(
                listed.contains(house_name),
                "external config names House {house_name:?} absent from map [Houses]"
            );
            if let Some(value) = fields.player_control {
                values.push((
                    house_name.clone(),
                    "PlayerControl".to_owned(),
                    bool_text(value).to_owned(),
                ));
            }
            for (index, value) in fields.player_controls.iter().enumerate() {
                if let Some(value) = value {
                    values.push((
                        house_name.clone(),
                        format!("Player{}Control", index + 1),
                        bool_text(*value).to_owned(),
                    ));
                }
            }
        }

        let mut projected = map_ini.clone();
        if self.replace_map_values {
            let mut removals = vec![
                ("Basic".to_owned(), "Player".to_owned()),
                ("Basic".to_owned(), "Player1".to_owned()),
                ("Basic".to_owned(), "Player2".to_owned()),
                ("Basic".to_owned(), "Player3".to_owned()),
                ("Basic".to_owned(), "Player4".to_owned()),
            ];
            for house in &listed {
                removals.push((house.clone(), "PlayerControl".to_owned()));
                for index in 1..=4 {
                    removals.push((house.clone(), format!("Player{index}Control")));
                }
            }
            projected.remove_source_values(
                removals
                    .iter()
                    .map(|(section, key)| (section.as_str(), key.as_str())),
            );
        }
        projected.merge_source_values(
            values
                .iter()
                .map(|(section, key, value)| (section.as_str(), key.as_str(), value.as_str())),
            true,
        );
        Ok(projected)
    }

    /// Mark this normalized value as the authoritative campaign scenario
    /// config, clearing all cooperative fields authored in the map first.
    pub fn replace_map_cooperative_values(&mut self) {
        self.replace_map_values = true;
    }
}

fn listed_houses(map_ini: &IniFile) -> BTreeSet<String> {
    let Some(section) = map_ini.section("Houses") else {
        return BTreeSet::new();
    };
    section
        .keys()
        .filter_map(|key| section.read_name(key, 0x14))
        .map(str::to_owned)
        .collect()
}

fn validate_supplemental(supplemental: &IniFile, houses: &BTreeSet<String>) -> anyhow::Result<()> {
    for section in supplemental.physical_sections() {
        for key in section.physical_keys_including_explicit_empty() {
            if section.name == "Basic" {
                anyhow::ensure!(
                    matches!(key, "Player1" | "Player2" | "Player3" | "Player4")
                        || (houses.contains("Basic") && is_house_control_key(key)),
                    "external config key [{0}] {key} is not allowed",
                    section.name
                );
            } else {
                anyhow::ensure!(
                    houses.contains(&section.name),
                    "external config House section {:?} is absent from map [Houses]",
                    section.name
                );
                anyhow::ensure!(
                    is_house_control_key(key),
                    "external config key [{0}] {key} is not allowed",
                    section.name
                );
            }
        }
    }
    Ok(())
}

fn is_house_control_key(key: &str) -> bool {
    key == "PlayerControl"
        || matches!(
            key,
            "Player1Control" | "Player2Control" | "Player3Control" | "Player4Control"
        )
}

fn read_control(
    section: &crate::rules::ini_parser::IniSection,
    key: &str,
) -> anyhow::Result<Option<bool>> {
    if !section.is_present(key) {
        return Ok(None);
    }
    section
        .read_bool_value(key)
        .map(Some)
        .ok_or_else(|| anyhow::anyhow!("invalid external House control value for {key}"))
}

fn validate_player_name(value: &str) -> anyhow::Result<()> {
    if value.is_empty() {
        // An explicit empty ReadString name falls through to Basic.Player=.
        return Ok(());
    }
    anyhow::ensure!(
        !value
            .chars()
            .any(|character| character.is_control() || character == ';'),
        "external Basic player name is not a canonical INI name"
    );
    let fixture = IniFile::from_str(&format!("[Basic]\nPlayer1={value}\n"));
    anyhow::ensure!(
        fixture
            .section("Basic")
            .and_then(|section| section.read_name("Player1", 0x14))
            == Some(value),
        "external Basic player name exceeds the native reader format"
    );
    Ok(())
}

fn bool_text(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::basic::parse_basic_section;
    use crate::map::houses::parse_house_roster;

    fn map_ini() -> IniFile {
        IniFile::from_str(
            "[Basic]\nPlayer1=Alpha\n[Houses]\n0=Alpha\n1=Beta\n[Alpha]\nPlayerControl=no\nPlayer1Control=yes\n[Beta]\n",
        )
    }

    #[test]
    fn coop_config_projects_only_allowed_values_and_keeps_player1_control_precedence() {
        let map = map_ini();
        let supplemental = IniFile::from_str(
            "[Basic]\nPlayer2=Beta\n[Alpha]\nPlayerControl=no\nPlayer2Control=yes\n",
        );
        let config = VeraCoopMapConfig::from_ini(&map, &supplemental).unwrap();
        let projected = config.apply_to_map(&map).unwrap();

        assert_eq!(
            parse_basic_section(&projected).campaign_players.players[1]
                .owner
                .as_deref(),
            Some("Beta")
        );
        let roster = parse_house_roster(&projected, &[], None);
        assert_eq!(roster.houses[0].campaign_player_controls[0], Some(true));
        assert_eq!(roster.houses[0].campaign_player_controls[1], Some(true));
        assert!(projected.section("Basic").unwrap().is_present("Player1"));
        assert!(!projected.section("Basic").unwrap().is_present("Player3"));
    }

    #[test]
    fn coop_config_rejects_unknown_house_and_disallowed_keys() {
        let map = map_ini();
        let unknown_house = IniFile::from_str("[Gamma]\nPlayerControl=yes\n");
        assert!(VeraCoopMapConfig::from_ini(&map, &unknown_house).is_err());

        let disallowed = IniFile::from_str("[Basic]\nName=not allowed\n");
        assert!(VeraCoopMapConfig::from_ini(&map, &disallowed).is_err());
        let unapproved_basic_house_field = IniFile::from_str("[Basic]\nCountry=not allowed\n");
        assert!(VeraCoopMapConfig::from_ini(&map, &unapproved_basic_house_field).is_err());
        let config: VeraCoopMapConfig = serde_json::from_str(
            r#"{"basic_players":[null,null,null,null],"houses":{"Gamma":{"player_control":true,"player_controls":[null,null,null,null]}}}"#,
        )
        .unwrap();
        assert!(config.apply_to_map(&map_ini()).is_err());
    }

    #[test]
    fn basic_section_can_also_supply_controls_when_basic_is_a_listed_house() {
        let map = IniFile::from_str(
            "[Basic]\nPlayer1=Alpha\nPlayerControl=no\nPlayer1Control=yes\n[Houses]\n0=Basic\n1=Alpha\n[Alpha]\n",
        );
        let config = VeraCoopMapConfig::from_bytes(
            &map,
            b"[Basic]\nPlayer2=Alpha\nPlayerControl=yes\nPlayer1Control=no",
        )
        .unwrap();
        let projected = config.apply_to_map(&map).unwrap();

        assert_eq!(
            parse_basic_section(&projected).campaign_players.players[1]
                .owner
                .as_deref(),
            Some("Alpha")
        );
        assert!(
            VeraCoopMapConfig::from_bytes(
                &map,
                b"[Basic]\nPlayer2=Alpha\nPlayerControl=yes\nCountry=not allowed"
            )
            .is_err()
        );

        let roster = parse_house_roster(&projected, &[], None);
        let basic_house = &roster.houses[0];
        assert_eq!(basic_house.player_control, Some(true));
        assert_eq!(basic_house.campaign_player_controls[0], Some(false));
    }

    #[test]
    fn explicit_empty_player1_clears_map_override_and_falls_back_to_legacy_player() {
        let map = IniFile::from_str(
            "[Basic]\nPlayer=Beta\nPlayer1=Alpha\n[Houses]\n0=Alpha\n1=Beta\n[Alpha]\n[Beta]\n",
        );
        let ordinary = IniFile::from_bytes(b"[Basic]\nPlayer1=\n").unwrap();
        assert!(ordinary.section("Basic").is_none());

        let config = VeraCoopMapConfig::from_bytes(&map, b"[Basic]\nPlayer1=\n").unwrap();
        let projected = config.apply_to_map(&map).unwrap();
        assert_eq!(
            parse_basic_section(&projected).campaign_players.players[0]
                .owner
                .as_deref(),
            Some("Beta")
        );
    }
}
