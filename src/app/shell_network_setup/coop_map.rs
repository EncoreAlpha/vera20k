//! The selected LAN host's external VERA Coop map settings.

use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::app::AppState;
use crate::app::match_bootstrap::NetworkVeraCoopMapConfig;
use crate::map::source::{LoadedMapIni, LoadedMapSource, load_network_map_ini};
use crate::rules::ini_parser::IniFile;

const CAMPAIGN_ROOT: &str = "VeraCoop";
const CAMPAIGN_FILE: &str = "campaign.ini";
const MAX_CAMPAIGN_BYTES: usize = 64 * 1024;

/// Resolve a map only through a discovered external campaign manifest.
/// Maps are rooted at `settings/Maps/VeraCoop`, independent of the campaign
/// directory containing `campaign.ini`.
pub(crate) fn resolve_host_vera_coop_map_config(
    state: &AppState,
    map_file: &str,
) -> Result<NetworkVeraCoopMapConfig> {
    crate::map::source::validate_network_map_name(map_file)?;
    let game_config = state
        .platform
        .game_config
        .as_ref()
        .context("VERA Coop campaign requires profile paths")?;
    let assets = state
        .process_assets
        .manager()
        .context("VERA Coop map config requires active retail assets")?;
    let maps_root = game_config.paths.settings_dir().join("Maps");
    let coop_root = maps_root.join(CAMPAIGN_ROOT);
    let campaign_map = map_path_from_catalog(map_file)?;
    let loaded = load_campaign_map_ini(&maps_root, &coop_root, &campaign_map, assets)?;
    let source_sha256 = loaded
        .source
        .physical_identity()
        .map(|(_, digest)| digest.to_owned())
        .context("VERA Coop requires a physical authored map")?;
    let config = load_campaign_scenario(&coop_root, &campaign_map, &loaded.ini)?;
    let (directory, campaign_sha256) = host_campaign_identity(state, map_file, &source_sha256)?;
    NetworkVeraCoopMapConfig::new(map_file.to_owned(), source_sha256, config)?
        .with_campaign_identity(directory, campaign_sha256)
}

/// Resolve the host's full campaign identity for the LAN options snapshot.
/// The campaign bytes remain local; only the containing folder name and hash
/// are published to guests.
pub(crate) fn host_campaign_identity(
    state: &AppState,
    map_file: &str,
    expected_source_sha256: &str,
) -> Result<(String, String)> {
    let (maps_root, coop_root) = campaign_roots(state)?;
    let campaign_map = map_path_from_catalog(map_file)?;
    let root = coop_root.canonicalize()?;
    let mut matches = Vec::new();
    for entry in std::fs::read_dir(&root).context("scan VERA Coop campaigns")? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join(CAMPAIGN_FILE);
        let Ok(path) = path.canonicalize() else {
            continue;
        };
        if !path.starts_with(&root) {
            continue;
        }
        let Ok(bytes) = read_bounded(&path) else {
            continue;
        };
        let Ok(ini) = IniFile::from_bytes_preserving_empty_values(&bytes) else {
            continue;
        };
        let Ok(manifest) = parse_manifest(&ini) else {
            continue;
        };
        if validate_manifest_maps(&root, &manifest.maps).is_err()
            || !manifest
                .maps
                .iter()
                .any(|path| same_map_name(path, &campaign_map))
        {
            continue;
        }
        // Confirm the selected map resolves to the listed loose file. This
        // binds the campaign digest to the same physical map identity as the
        // ordinary LAN map metadata.
        let loaded = load_campaign_map_ini(
            &maps_root,
            &coop_root,
            &campaign_map,
            state
                .process_assets
                .manager()
                .context("VERA Coop map config requires active retail assets")?,
        )?;
        ensure!(
            loaded
                .source
                .physical_identity()
                .is_some_and(|(_, digest)| digest == expected_source_sha256),
            "host VERA Coop map changed after selection"
        );
        matches.push((entry.file_name().to_string_lossy().into_owned(), bytes));
    }
    ensure!(
        matches.len() == 1,
        "selected map does not belong to exactly one valid VERA Coop campaign"
    );
    let (directory, bytes) = matches.pop().expect("one campaign match");
    Ok((directory, crate::util::sha256::sha256_hex(&bytes)))
}

/// Resolve the guest's preinstalled campaign by the host-advertised folder.
/// Every manifest map must already exist, and the selected map must be the
/// exact loose file under Maps/VeraCoop with the host's physical digest.
pub(crate) fn resolve_local_campaign_config(
    state: &AppState,
    map_file: &str,
    directory: &str,
    expected_source_sha256: &str,
) -> Result<(NetworkVeraCoopMapConfig, String)> {
    let component = Path::new(directory);
    ensure!(
        !directory.is_empty()
            && component.components().count() == 1
            && component
                .components()
                .next()
                .is_some_and(|part| matches!(part, std::path::Component::Normal(_))),
        "invalid VERA Coop campaign directory"
    );
    crate::map::source::validate_network_map_name(map_file)?;
    let (maps_root, coop_root) = campaign_roots(state)?;
    let campaign_map = map_path_from_catalog(map_file)?;
    let assets = state
        .process_assets
        .manager()
        .context("VERA Coop map config requires active retail assets")?;
    let loaded = load_campaign_map_ini(&maps_root, &coop_root, &campaign_map, assets)?;
    let source_sha256 = loaded
        .source
        .physical_identity()
        .map(|(_, digest)| digest.to_owned())
        .context("VERA Coop requires a physical authored map")?;
    ensure!(
        source_sha256 == expected_source_sha256,
        "local VERA Coop map differs from host map"
    );
    let root = coop_root.canonicalize()?;
    let campaign_path = root
        .join(directory)
        .join(CAMPAIGN_FILE)
        .canonicalize()
        .context("local VERA Coop campaign.ini is missing")?;
    ensure!(
        campaign_path.starts_with(&root),
        "campaign.ini escapes VERA Coop root"
    );
    let bytes = read_bounded(&campaign_path)?;
    let ini = IniFile::from_bytes_preserving_empty_values(&bytes).map_err(anyhow::Error::from)?;
    let manifest = parse_manifest(&ini)?;
    validate_manifest_maps(&root, &manifest.maps)?;
    ensure!(
        manifest
            .maps
            .iter()
            .any(|path| same_map_name(path, &campaign_map)),
        "local campaign does not list the host's selected map"
    );
    let config = load_campaign_scenario(&coop_root, &campaign_map, &loaded.ini)?;
    let campaign_sha256 = crate::util::sha256::sha256_hex(&bytes);
    let config = NetworkVeraCoopMapConfig::new(map_file.to_owned(), source_sha256, config)?
        .with_campaign_identity(directory.to_owned(), campaign_sha256.clone())?;
    Ok((config, campaign_sha256))
}

fn next_scenario_token(ini: &IniFile, map_file: &str) -> Result<Option<String>> {
    let normalized = normalize_map_name(map_file)?;
    let section_name = format!("Scenario:{normalized}");
    let section = ini
        .section(&section_name)
        .with_context(|| format!("campaign is missing [{section_name}]"))?;
    if section.read_bool_value("EndOfGame").unwrap_or(false) {
        return Ok(None);
    }
    let next = section
        .read_string("NextScenario", "", 1024)
        .trim()
        .to_owned();
    if next.is_empty() {
        return Ok(None);
    }
    crate::map::source::validate_network_map_name(&next)
        .with_context(|| format!("invalid [{section_name}] NextScenario"))?;
    Ok(Some(next))
}

/// Resolve the next stage for the current VERA Coop campaign map. Both
/// peers resolve locally from the already-admitted campaign identity; this
/// function never transfers a map. `AltNextScenario` is left for the
/// `SkipMapSelect`/global-variable branch.
pub(crate) fn resolve_next_campaign_stage(
    state: &AppState,
    current: &crate::app::match_bootstrap::NetworkVeraCoopMapConfig,
) -> Result<Option<crate::app::match_bootstrap::VeraCoopNextStage>> {
    let Some(directory) = current.campaign_directory.as_deref() else {
        return Ok(None);
    };
    let Some(expected_sha256) = current.campaign_sha256.as_deref() else {
        return Ok(None);
    };
    let (maps_root, coop_root) = campaign_roots(state)?;
    let campaign_path = coop_root.join(directory).join(CAMPAIGN_FILE);
    let bytes = read_bounded(&campaign_path)?;
    ensure!(
        crate::util::sha256::sha256_hex(&bytes) == expected_sha256,
        "local campaign.ini differs from the admitted campaign identity"
    );
    let ini = IniFile::from_bytes_preserving_empty_values(&bytes)?;
    let manifest = parse_manifest(&ini)?;
    let canonical_root = coop_root.canonicalize()?;
    validate_manifest_maps(&canonical_root, &manifest.maps)?;
    let Some(target) = next_scenario_token(&ini, &map_path_from_catalog(&current.map_file)?)?
    else {
        return Ok(None);
    };
    ensure!(
        manifest.maps.iter().any(|map| same_map_name(map, &target)),
        "campaign NextScenario is absent from ScenarioCount"
    );
    let assets = state
        .process_assets
        .manager()
        .context("VERA Coop next stage requires active retail assets")?;
    let loaded = load_campaign_map_ini(&maps_root, &coop_root, &target, assets)?;
    let (byte_length, source_sha256) = loaded
        .source
        .physical_identity()
        .map(|(length, digest)| (length, digest.to_owned()))
        .context("VERA Coop next stage requires a physical authored map")?;
    let effective_digest = crate::map::source::effective_network_map_digest(&loaded.ini, false);
    let display_name = loaded
        .ini
        .section("Basic")
        .map(|section| section.read_string("Name", "", 0x20))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| target.clone());
    let config = load_campaign_scenario(&coop_root, &target, &loaded.ini)?;
    let next = crate::app::match_bootstrap::NetworkVeraCoopMapConfig::new(
        target.clone(),
        source_sha256.clone(),
        config,
    )?
    .with_campaign_identity(directory.to_owned(), expected_sha256.to_owned())?;
    let metadata = crate::app::match_runtime::session::NetworkMapMetadata {
        display_name,
        player_capacity: 8,
        official: false,
        byte_length: i32::try_from(byte_length).unwrap_or(i32::MAX),
        digest: String::new(),
        source_sha256,
        effective_digest: effective_digest.clone(),
    };
    Ok(Some(crate::app::match_bootstrap::VeraCoopNextStage {
        map_file: target,
        config: next,
        metadata,
        local_map_source: loaded.source,
        effective_digest,
    }))
}

fn campaign_roots(state: &AppState) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    let game_config = state
        .platform
        .game_config
        .as_ref()
        .context("VERA Coop campaign requires profile paths")?;
    let maps_root = game_config.paths.settings_dir().join("Maps");
    let coop_root = maps_root.join(CAMPAIGN_ROOT);
    Ok((maps_root, coop_root))
}

/// Refresh validated campaign Entry maps in the shared scenario catalog.
/// Their source marker limits them to the VERA Coop map filter.
pub(crate) fn refresh_campaign_entry_records(state: &mut AppState) {
    {
        let mut records = state.frontend.scenario_catalog.records_mut();
        records.retain(|record| {
            !matches!(
                &record.source,
                crate::map::skirmish_scenarios::SkirmishScenarioSource::VeraCoopCampaign(_)
            )
        });
    }
    let Some(game_config) = state.platform.game_config.as_ref() else {
        return;
    };
    let maps_root = game_config.paths.settings_dir().join("Maps");
    let coop_root = maps_root.join(CAMPAIGN_ROOT);
    let Ok(canonical_root) = coop_root.canonicalize() else {
        return;
    };
    let Some(assets) = state.process_assets.manager() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&canonical_root) else {
        return;
    };
    let mut discovered = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let path = entry.path().join(CAMPAIGN_FILE);
        let Ok(canonical_campaign) = path.canonicalize() else {
            continue;
        };
        if !canonical_campaign.starts_with(&canonical_root) {
            log::warn!(
                "ignoring VERA Coop campaign INI outside campaign root: {}",
                path.display()
            );
            continue;
        }
        let parsed = read_bounded(&canonical_campaign)
            .and_then(|bytes| {
                IniFile::from_bytes_preserving_empty_values(&bytes).map_err(anyhow::Error::from)
            })
            .and_then(|ini| {
                let manifest = parse_manifest(&ini)?;
                validate_manifest_maps(&canonical_root, &manifest.maps)?;
                Ok((ini, manifest))
            });
        let (_ini, manifest) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                log::warn!(
                    "ignoring invalid VERA Coop campaign {}: {error:#}",
                    path.display()
                );
                continue;
            }
        };
        let logical_name = format!("{CAMPAIGN_ROOT}/{}", manifest.entry);
        let Ok(loaded) = load_campaign_map_ini(&maps_root, &coop_root, &manifest.entry, assets)
        else {
            continue;
        };
        let display_name = entry.file_name().to_string_lossy().into_owned();
        let mut record = crate::map::skirmish_scenarios::SkirmishScenarioRecord::concrete_from_ini(
            discovered.len(),
            crate::map::skirmish_scenarios::SkirmishScenarioSource::VeraCoopCampaign(display_name),
            &logical_name,
            &loaded.ini,
        );
        if load_campaign_scenario(&coop_root, &manifest.entry, &loaded.ini).is_err() {
            log::warn!(
                "ignoring VERA Coop campaign {} with invalid entry configuration",
                entry.path().display()
            );
            continue;
        }
        record.game_modes.clear();
        discovered.push(record);
    }
    let mut path_counts = std::collections::BTreeMap::<String, usize>::new();
    for record in &discovered {
        *path_counts
            .entry(record.file_name.to_ascii_lowercase())
            .or_default() += 1;
    }
    discovered.retain(|record| {
        path_counts
            .get(&record.file_name.to_ascii_lowercase())
            .copied()
            == Some(1)
    });
    log::debug!(
        "VERA Coop campaign discovery: {} record(s)",
        discovered.len()
    );
    let mut records = state.frontend.scenario_catalog.records_mut();
    records.extend(discovered);
}

fn load_campaign_map_ini(
    maps_root: &Path,
    coop_root: &Path,
    campaign_map: &str,
    assets: &crate::assets::asset_manager::AssetManager,
) -> Result<LoadedMapIni> {
    crate::map::source::validate_network_map_name(campaign_map)?;
    let expected = coop_root
        .join(campaign_map)
        .canonicalize()
        .with_context(|| format!("campaign map is missing: {campaign_map}"))?;
    ensure!(
        expected.starts_with(coop_root.canonicalize()?),
        "campaign map escapes VERA Coop root"
    );
    let logical_name = format!("{CAMPAIGN_ROOT}/{campaign_map}");
    let loaded = load_network_map_ini(maps_root, &logical_name, assets)?;
    validate_campaign_source(&loaded.source, &expected)?;
    Ok(loaded)
}

fn validate_campaign_source(source: &LoadedMapSource, expected: &Path) -> Result<()> {
    let LoadedMapSource::Loose { path, .. } = source else {
        anyhow::bail!("VERA Coop campaign maps must resolve to loose files under Maps/VeraCoop")
    };
    let actual = path.canonicalize()?;
    ensure!(
        actual == expected,
        "VERA Coop map source does not match its campaign manifest path"
    );
    Ok(())
}

pub(crate) fn load_campaign_scenario(
    coop_root: &Path,
    map_file: &str,
    map_ini: &IniFile,
) -> Result<crate::map::coop_config::VeraCoopMapConfig> {
    crate::map::source::validate_network_map_name(map_file)?;
    let canonical_root = coop_root
        .canonicalize()
        .context("VERA Coop campaign root is missing")?;
    let mut matches = Vec::new();
    for entry in std::fs::read_dir(&canonical_root).context("scan VERA Coop campaigns")? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let campaign_path = entry.path().join(CAMPAIGN_FILE);
        if !campaign_path.is_file() {
            continue;
        }
        let parsed = campaign_path
            .canonicalize()
            .map_err(anyhow::Error::from)
            .and_then(|path| {
                ensure!(
                    path.starts_with(&canonical_root),
                    "campaign INI escapes the VERA Coop root"
                );
                let bytes = read_bounded(&path)?;
                let ini = IniFile::from_bytes_preserving_empty_values(&bytes)
                    .map_err(anyhow::Error::from)?;
                let manifest = parse_manifest(&ini)?;
                validate_manifest_maps(&canonical_root, &manifest.maps)?;
                Ok((ini, manifest))
            });
        let (ini, manifest) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                log::warn!(
                    "ignoring invalid VERA Coop campaign {}: {error:#}",
                    campaign_path.display()
                );
                continue;
            }
        };
        if manifest
            .maps
            .iter()
            .any(|path| same_map_name(path, map_file))
        {
            matches.push((ini, manifest));
        }
    }
    ensure!(
        matches.len() == 1,
        "selected VERA Coop map must belong to exactly one valid external campaign"
    );
    let (ini, manifest) = matches.pop().expect("one campaign match");
    let normalized = normalize_map_name(map_file)?;
    let section_name = format!("Scenario:{normalized}");
    let section = ini
        .section(&section_name)
        .with_context(|| format!("campaign is missing [{section_name}]"))?;
    let player_count = section.read_int("PlayerCount", 0);
    ensure!(
        (1..=4).contains(&player_count),
        "campaign PlayerCount must be 1 through 4"
    );
    for key in ["NextScenario", "AltNextScenario"] {
        if section.is_present(key) {
            let target = section.read_string(key, "", 1024).trim().to_owned();
            if !target.is_empty() {
                crate::map::source::validate_network_map_name(&target)
                    .with_context(|| format!("invalid [{section_name}] {key}"))?;
                ensure!(
                    manifest.maps.iter().any(|map| same_map_name(map, &target)),
                    "campaign [{section_name}] {key} target is absent from ScenarioCount"
                );
            }
        }
    }
    if section.is_present("EndOfGame") {
        section
            .read_bool_value("EndOfGame")
            .context("invalid campaign EndOfGame value")?;
    }
    for key in section.physical_keys_including_explicit_empty() {
        ensure!(
            key == "PlayerCount"
                || matches!(key, "Player1" | "Player2" | "Player3" | "Player4")
                || matches!(key, "NextScenario" | "AltNextScenario" | "EndOfGame"),
            "unsupported campaign scenario key [{section_name}] {key}"
        );
    }
    for index in player_count + 1..=4 {
        let key = format!("Player{index}");
        ensure!(
            !section.is_present(&key) && !section.has_explicit_empty_value(&key),
            "campaign [{section_name}] defines {key} beyond PlayerCount"
        );
    }
    let listed: std::collections::BTreeSet<_> = listed_houses(map_ini).into_iter().collect();
    let control_prefix = format!("{section_name}:");
    for control_section_name in ini.section_names() {
        let Some(house) = control_section_name.strip_prefix(&control_prefix) else {
            continue;
        };
        ensure!(
            listed.contains(house),
            "campaign control section [{control_section_name}] names a House absent from map [Houses]"
        );
        if let Some(controls) = ini.section(control_section_name) {
            for key in controls.physical_keys_including_explicit_empty() {
                ensure!(
                    key == "PlayerControl"
                        || matches!(
                            key,
                            "Player1Control"
                                | "Player2Control"
                                | "Player3Control"
                                | "Player4Control"
                        ),
                    "unsupported campaign House control key [{control_section_name}] {key}"
                );
                ensure!(
                    !controls.has_explicit_empty_value(key),
                    "empty campaign House control value [{control_section_name}] {key}"
                );
            }
        }
    }
    let mut text = String::from("[Basic]\n");
    let mut main_houses = std::collections::BTreeSet::new();
    for index in 1..=4 {
        let key = format!("Player{index}");
        if index <= player_count {
            let house = section
                .read_name(&key, 0x14)
                .with_context(|| format!("campaign [{section_name}] requires {key}"))?;
            ensure!(
                main_houses.insert(house.to_owned()),
                "campaign primary Houses must be distinct"
            );
            text.push_str(&format!("{key}={house}\n"));
        } else {
            // Empty explicit values overwrite any legacy map-authored coop slot.
            text.push_str(&format!("{key}=\n"));
        }
    }
    for house in listed_houses(map_ini) {
        let control_section_name = format!("Scenario:{normalized}:{house}");
        if let Some(controls) = ini.section(&control_section_name) {
            text.push_str(&format!("[{house}]\n"));
            for index in 1..=4 {
                let key = format!("Player{index}Control");
                if controls.is_present(&key) {
                    let value = controls
                        .read_bool_value(&key)
                        .with_context(|| format!("invalid [{control_section_name}] {key}"))?;
                    text.push_str(&format!("{key}={}\n", if value { "yes" } else { "no" }));
                }
            }
            if controls.is_present("PlayerControl") {
                let value = controls
                    .read_bool_value("PlayerControl")
                    .with_context(|| format!("invalid [{control_section_name}] PlayerControl"))?;
                text.push_str(&format!(
                    "PlayerControl={}\n",
                    if value { "yes" } else { "no" }
                ));
            }
        }
    }
    let external = IniFile::from_bytes_preserving_empty_values(text.as_bytes())
        .map_err(anyhow::Error::from)?;
    let mut config = crate::map::coop_config::VeraCoopMapConfig::from_ini(map_ini, &external)?;
    config.replace_map_cooperative_values();
    config.apply_to_map(map_ini)?;
    ensure!(
        manifest
            .maps
            .iter()
            .any(|map| same_map_name(map, &manifest.entry)),
        "campaign entry is invalid"
    );
    Ok(config)
}

struct CampaignManifest {
    entry: String,
    maps: Vec<String>,
}

fn parse_manifest(ini: &IniFile) -> Result<CampaignManifest> {
    ensure!(
        !ini.has_duplicate_sections() && !ini.has_duplicate_keys(),
        "campaign INI contains repeated physical sections or keys"
    );
    let section = ini
        .section("Campaign")
        .context("campaign INI requires [Campaign]")?;
    let entry = section.read_string("Entry", "", 1024).trim().to_owned();
    crate::map::source::validate_network_map_name(&entry).context("invalid campaign Entry")?;
    let count = section.read_int("ScenarioCount", 0);
    ensure!(
        (1..=512).contains(&count),
        "campaign ScenarioCount must be 1 through 512"
    );
    let mut maps: Vec<String> = Vec::with_capacity(count as usize);
    for index in 1..=count {
        let key = format!("Scenario{index}");
        let map = section.read_string(&key, "", 1024).trim().to_owned();
        crate::map::source::validate_network_map_name(&map)
            .with_context(|| format!("invalid Campaign {key}"))?;
        ensure!(
            !maps.iter().any(|known| same_map_name(known, &map)),
            "duplicate campaign scenario {map:?}"
        );
        maps.push(map);
    }
    ensure!(
        maps.iter().any(|map| same_map_name(map, &entry)),
        "campaign Entry is absent from scenario list"
    );
    Ok(CampaignManifest { entry, maps })
}

fn validate_manifest_maps(root: &Path, maps: &[String]) -> Result<()> {
    for map in maps {
        let relative = Path::new(&map.replace('\\', "/")).to_path_buf();
        let path = root.join(relative);
        let canonical = path
            .canonicalize()
            .with_context(|| format!("campaign map is missing: {map}"))?;
        ensure!(
            canonical.starts_with(root),
            "campaign map escapes the VERA Coop root: {map}"
        );
        ensure!(canonical.is_file(), "campaign map is not a file: {map}");
    }
    Ok(())
}

fn listed_houses(map_ini: &IniFile) -> Vec<String> {
    map_ini
        .section("Houses")
        .into_iter()
        .flat_map(|section| {
            section
                .keys()
                .filter_map(|key| section.read_name(key, 0x14).map(str::to_owned))
        })
        .collect()
}

fn normalize_map_name(name: &str) -> Result<String> {
    crate::map::source::validate_network_map_name(name)?;
    Ok(name.replace('\\', "/"))
}

fn map_path_from_catalog(name: &str) -> Result<String> {
    let normalized = normalize_map_name(name)?;
    if normalized
        .get(.."VeraCoop/".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("VeraCoop/"))
    {
        Ok(normalized["VeraCoop/".len()..].to_owned())
    } else {
        Ok(normalized)
    }
}

fn same_map_name(left: &str, right: &str) -> bool {
    left.replace('\\', "/")
        .eq_ignore_ascii_case(&right.replace('\\', "/"))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    #[cfg(not(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown")))]
    {
        use std::io::Read;
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take((MAX_CAMPAIGN_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= MAX_CAMPAIGN_BYTES,
            "VERA Coop campaign INI exceeds 64 KiB"
        );
        Ok(bytes)
    }
    #[cfg(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown"))]
    {
        let bytes = crate::util::file_system::read_shared(path)?;
        ensure!(
            bytes.len() <= MAX_CAMPAIGN_BYTES,
            "VERA Coop campaign INI exceeds 64 KiB"
        );
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_manifest_requires_entry_and_rejects_duplicate_or_escaping_paths() {
        let valid = IniFile::from_str(
            "[Campaign]\nEntry=Allied/01.map\nScenarioCount=2\nScenario1=Allied/01.map\nScenario2=Soviet/02.map\n",
        );
        let manifest = parse_manifest(&valid).unwrap();
        assert_eq!(manifest.entry, "Allied/01.map");
        assert_eq!(manifest.maps.len(), 2);

        let duplicate = IniFile::from_str(
            "[Campaign]\nEntry=01.map\nScenarioCount=2\nScenario1=01.map\nScenario2=01.MAP\n",
        );
        assert!(parse_manifest(&duplicate).is_err());
        let escaping = IniFile::from_str(
            "[Campaign]\nEntry=../01.map\nScenarioCount=1\nScenario1=../01.map\n",
        );
        assert!(parse_manifest(&escaping).is_err());
        let duplicate_section = IniFile::from_str(
            "[Campaign]\nEntry=01.map\nScenarioCount=1\nScenario1=01.map\n[Campaign]\nScenarioCount=1\n",
        );
        assert!(parse_manifest(&duplicate_section).is_err());
    }

    #[test]
    fn campaign_scenario_projects_explicit_roles_and_controls_without_map_fallback() {
        let directory =
            std::env::temp_dir().join(format!("vera-coop-campaign-{}", std::process::id()));
        let campaign_dir = directory.join("campaign-a");
        std::fs::create_dir_all(campaign_dir.join("Allied")).unwrap();
        std::fs::create_dir_all(campaign_dir.join("Soviet")).unwrap();
        std::fs::write(campaign_dir.join("Allied/01.map"), b"map").unwrap();
        std::fs::write(campaign_dir.join("Soviet/02.map"), b"map").unwrap();
        std::fs::write(
            campaign_dir.join(CAMPAIGN_FILE),
            b"[Campaign]\nEntry=campaign-a/Allied/01.map\nScenarioCount=2\nScenario1=campaign-a/Allied/01.map\nScenario2=campaign-a/Soviet/02.map\n[Scenario:campaign-a/Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\nNextScenario=campaign-a/Soviet/02.map\n[Scenario:campaign-a/Allied/01.map:Neutral]\nPlayer1Control=yes\nPlayer2Control=no\n",
        ).unwrap();
        let coop_root = directory.canonicalize().unwrap();
        let map = IniFile::from_str(
            "[Basic]\nPlayer=Legacy\nPlayer1=OldA\nPlayer2=OldB\nPlayer3=OldC\n[Houses]\n0=Alpha\n1=Beta\n2=Neutral\n[Alpha]\n[Beta]\n[Neutral]\nPlayerControl=yes\n",
        );
        let config = load_campaign_scenario(&coop_root, "campaign-a/Allied/01.map", &map).unwrap();
        let projected = config.apply_to_map(&map).unwrap();
        let basic = crate::map::basic::parse_basic_section(&projected);
        assert_eq!(
            basic.campaign_players.players[0].owner.as_deref(),
            Some("Alpha")
        );
        assert_eq!(
            basic.campaign_players.players[1].owner.as_deref(),
            Some("Beta")
        );
        assert_eq!(basic.campaign_players.players[2].owner, None);
        let houses = crate::map::houses::parse_house_roster(&projected, &[], None);
        let neutral = houses
            .houses
            .iter()
            .find(|house| house.name == "Neutral")
            .unwrap();
        assert_eq!(neutral.campaign_player_controls[0], Some(true));
        assert_eq!(neutral.campaign_player_controls[1], Some(false));
        assert_eq!(neutral.player_control, None);

        let campaign_path = campaign_dir.join(CAMPAIGN_FILE);
        std::fs::write(
            &campaign_path,
            b"[Campaign]\nEntry=campaign-a/Allied/01.map\nScenarioCount=2\nScenario1=campaign-a/Allied/01.map\nScenario2=campaign-a/Soviet/02.map\n[Scenario:campaign-a/Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\nPlayer3=Neutral\n",
        )
        .unwrap();
        assert!(load_campaign_scenario(&coop_root, "campaign-a/Allied/01.map", &map).is_err());
        std::fs::write(
            &campaign_path,
            b"[Campaign]\nEntry=campaign-a/Allied/01.map\nScenarioCount=2\nScenario1=campaign-a/Allied/01.map\nScenario2=campaign-a/Soviet/02.map\n[Scenario:campaign-a/Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\nNextScenario=campaign-a/Soviet/02.map\nEndOfGame=maybe\n",
        )
        .unwrap();
        assert!(load_campaign_scenario(&coop_root, "campaign-a/Allied/01.map", &map).is_err());
        std::fs::write(
            &campaign_path,
            b"[Campaign]\nEntry=campaign-a/Allied/01.map\nScenarioCount=2\nScenario1=campaign-a/Allied/01.map\nScenario2=campaign-a/Soviet/02.map\n[Scenario:campaign-a/Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\nNextScenario=missing.map\n",
        )
        .unwrap();
        assert!(load_campaign_scenario(&coop_root, "campaign-a/Allied/01.map", &map).is_err());
        std::fs::write(
            &campaign_path,
            b"[Campaign]\nEntry=campaign-a/Allied/01.map\nScenarioCount=2\nScenario1=campaign-a/Allied/01.map\nScenario2=campaign-a/Soviet/02.map\n[Scenario:campaign-a/Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\nNextScenario=campaign-a/Soviet/02.map\nEndOfGame=maybe\n",
        )
        .unwrap();
        assert!(load_campaign_scenario(&coop_root, "campaign-a/Allied/01.map", &map).is_err());
        std::fs::write(
            &campaign_path,
            b"[Campaign]\nEntry=campaign-a/Allied/01.map\nScenarioCount=2\nScenario1=campaign-a/Allied/01.map\nScenario2=campaign-a/Soviet/02.map\n[Scenario:campaign-a/Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\n[Scenario:campaign-a/Allied/01.map:Ghost]\nPlayer1Control=yes\n",
        )
        .unwrap();
        assert!(load_campaign_scenario(&coop_root, "campaign-a/Allied/01.map", &map).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn campaign_manifest_requires_every_listed_map_to_exist_inside_root() {
        let directory =
            std::env::temp_dir().join(format!("vera-coop-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("present.map"), b"map").unwrap();
        let root = directory.canonicalize().unwrap();
        assert!(validate_manifest_maps(&root, &["present.map".to_owned()]).is_ok());
        assert!(validate_manifest_maps(&root, &["missing.map".to_owned()]).is_err());
        assert!(validate_manifest_maps(&root, &["../outside.map".to_owned()]).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn campaign_loader_rejects_retail_or_mix_source_for_manifest_path() {
        let directory =
            std::env::temp_dir().join(format!("vera-coop-source-{}", std::process::id()));
        let coop = directory.join("VeraCoop");
        std::fs::create_dir_all(&coop).unwrap();
        let expected = coop.join("entry.map");
        let impostor = directory.join("entry.map");
        std::fs::write(&expected, b"campaign").unwrap();
        std::fs::write(&impostor, b"retail").unwrap();
        let expected = expected.canonicalize().unwrap();
        let source = |path: std::path::PathBuf| LoadedMapSource::Loose {
            path,
            payload_len: 1,
            source_sha256: "a".repeat(64),
            cache_lease: Default::default(),
        };
        assert!(validate_campaign_source(&source(expected.clone()), &expected).is_ok());
        assert!(validate_campaign_source(&source(impostor), &expected).is_err());
        let archive = LoadedMapSource::Mix {
            logical_name: "VeraCoop/entry.map".to_owned(),
            source_archive: "ra2.mix".to_owned(),
            entry_id: 1,
            payload_len: 1,
            source_sha256: "a".repeat(64),
        };
        assert!(validate_campaign_source(&archive, &expected).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn next_scenario_token_prefers_end_of_game_and_requires_a_listed_target() {
        let ini = IniFile::from_str(
            "[Scenario:mission/01.map]\nNextScenario=mission/02.map\n[Scenario:mission/02.map]\nNextScenario=mission/01.map\nEndOfGame=yes\n",
        );
        assert_eq!(
            next_scenario_token(&ini, "mission/01.map")
                .unwrap()
                .as_deref(),
            Some("mission/02.map")
        );
        assert_eq!(next_scenario_token(&ini, "mission/02.map").unwrap(), None);
        let empty = IniFile::from_str("[Scenario:mission/03.map]\nPlayerCount=2\n");
        assert_eq!(next_scenario_token(&empty, "mission/03.map").unwrap(), None);
        let escaping =
            IniFile::from_str("[Scenario:mission/03.map]\nNextScenario=../outside.map\n");
        assert!(next_scenario_token(&escaping, "mission/03.map").is_err());
    }
}
