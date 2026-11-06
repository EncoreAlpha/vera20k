//! The selected LAN host's external VMCE map settings.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

use crate::app::AppState;
use crate::app::match_bootstrap::NetworkVmceMapConfig;
use crate::assets::csf_file::CsfFile;
use crate::map::source::{LoadedMapIni, LoadedMapSource, load_network_map_ini};
use crate::rules::ini_parser::IniFile;

/// One series campaign owns one direct subdirectory under this root. Its
/// `campaign.ini` and every manifest-relative map live inside that directory.
const VMCE_ROOT: &str = "VMCE";
const VMCE_MANIFEST_FILE: &str = "campaign.ini";
const VMCE_SERIES_CSF: &str = "ra2md.csf";
const MAX_CAMPAIGN_BYTES: usize = 64 * 1024;

/// Resolve a map only through a discovered external campaign manifest.
/// Maps are rooted at `settings/Maps/VMCE/<series>/`; manifest paths are
/// relative to that series directory.
pub(crate) fn resolve_host_vmce_map_config(
    state: &AppState,
    map_file: &str,
) -> Result<NetworkVmceMapConfig> {
    let (maps_root, vmce_root) = vmce_roots(state)?;
    let assets = state
        .process_assets
        .manager()
        .context("VMCE map config requires active retail assets")?;
    let (series, entry) = split_catalog_map(map_file)?;
    let series_root = series_root(&vmce_root, &series)?;
    let loaded = load_vmce_map_ini(&maps_root, &series_root, &series, &entry, assets)?;
    let source_sha256 = loaded
        .source
        .physical_identity()
        .map(|(_, digest)| digest.to_owned())
        .context("VMCE requires a physical authored map")?;
    let config = load_vmce_scenario(&series_root, &entry, &loaded.ini)?;
    let (directory, series_sha256) = host_vmce_identity(state, map_file, &source_sha256)?;
    ensure!(
        directory.eq_ignore_ascii_case(&series),
        "VMCE map changed its series directory after admission"
    );
    NetworkVmceMapConfig::new(catalog_map_file(&series, &entry), source_sha256, config)?
        .with_series_identity(directory, series_sha256)
}

/// Resolve the host's full series identity for the LAN options snapshot.
/// The campaign bytes remain local; only the containing series directory and
/// hash are published to guests.
pub(crate) fn host_vmce_identity(
    state: &AppState,
    map_file: &str,
    expected_source_sha256: &str,
) -> Result<(String, String)> {
    let (maps_root, vmce_root) = vmce_roots(state)?;
    let (series, entry) = split_catalog_map(map_file)?;
    let series_dir = series_root(&vmce_root, &series)?;
    let manifest_path = series_dir.join(VMCE_MANIFEST_FILE);
    let bytes = read_bounded(&manifest_path)?;
    let ini = IniFile::from_bytes_preserving_empty_values(&bytes)?;
    let manifest = parse_vmce_manifest(&ini)?;
    validate_vmce_manifest_maps(&series_dir, &manifest.maps)?;
    ensure!(
        manifest.maps.iter().any(|path| same_map_name(path, &entry)),
        "selected map is absent from its VMCE series manifest"
    );
    let assets = state
        .process_assets
        .manager()
        .context("VMCE map config requires active retail assets")?;
    let loaded = load_vmce_map_ini(&maps_root, &series_dir, &series, &entry, assets)?;
    ensure!(
        loaded
            .source
            .physical_identity()
            .is_some_and(|(_, digest)| digest == expected_source_sha256),
        "host VMCE map changed after selection"
    );
    Ok((series, crate::util::sha256::sha256_hex(&bytes)))
}

/// Resolve the guest's preinstalled campaign by the host-advertised folder.
/// Every manifest map must already exist, and the selected map must be the
/// exact loose file under Maps/Vmce with the host's physical digest.
pub(crate) fn resolve_local_vmce_config(
    state: &AppState,
    map_file: &str,
    directory: &str,
    expected_source_sha256: &str,
) -> Result<(NetworkVmceMapConfig, String)> {
    let component = Path::new(directory);
    ensure!(
        !directory.is_empty()
            && component.components().count() == 1
            && component
                .components()
                .next()
                .is_some_and(|part| matches!(part, std::path::Component::Normal(_))),
        "invalid VMCE series directory"
    );
    crate::map::source::validate_network_map_name(map_file)?;
    let (maps_root, vmce_root) = vmce_roots(state)?;
    let (map_series, entry) = split_catalog_map(map_file)?;
    ensure!(
        map_series.eq_ignore_ascii_case(directory),
        "host-advertised VMCE series does not match the selected map"
    );
    let series_dir = series_root(&vmce_root, directory)?;
    let assets = state
        .process_assets
        .manager()
        .context("VMCE map config requires active retail assets")?;
    let loaded = load_vmce_map_ini(&maps_root, &series_dir, directory, &entry, assets)?;
    let source_sha256 = loaded
        .source
        .physical_identity()
        .map(|(_, digest)| digest.to_owned())
        .context("VMCE requires a physical authored map")?;
    ensure!(
        source_sha256 == expected_source_sha256,
        "local VMCE map differs from host map"
    );
    let manifest_path = series_dir
        .join(VMCE_MANIFEST_FILE)
        .canonicalize()
        .context("local VMCE campaign.ini is missing")?;
    ensure!(
        manifest_path.starts_with(&series_dir),
        "campaign.ini escapes the VMCE series directory"
    );
    let bytes = read_bounded(&manifest_path)?;
    let ini = IniFile::from_bytes_preserving_empty_values(&bytes).map_err(anyhow::Error::from)?;
    let manifest = parse_vmce_manifest(&ini)?;
    validate_vmce_manifest_maps(&series_dir, &manifest.maps)?;
    ensure!(
        manifest.maps.iter().any(|path| same_map_name(path, &entry)),
        "local campaign does not list the host's selected map"
    );
    let config = load_vmce_scenario(&series_dir, &entry, &loaded.ini)?;
    let series_sha256 = crate::util::sha256::sha256_hex(&bytes);
    let config =
        NetworkVmceMapConfig::new(catalog_map_file(directory, &entry), source_sha256, config)?
            .with_series_identity(directory.to_owned(), series_sha256.clone())?;
    Ok((config, series_sha256))
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

/// Resolve the next stage for the current VMCE series map. Both
/// peers resolve locally from the already-admitted series identity; this
/// function never transfers a map. `AltNextScenario` is left for the
/// `SkipMapSelect`/global-variable branch.
pub(crate) fn resolve_next_vmce_stage(
    state: &AppState,
    current: &crate::app::match_bootstrap::NetworkVmceMapConfig,
) -> Result<Option<crate::app::match_bootstrap::VmceNextStage>> {
    let Some(directory) = current.series_directory.as_deref() else {
        return Ok(None);
    };
    let Some(expected_sha256) = current.series_sha256.as_deref() else {
        return Ok(None);
    };
    let (maps_root, vmce_root) = vmce_roots(state)?;
    let series_dir = series_root(&vmce_root, directory)?;
    let manifest_path = series_dir.join(VMCE_MANIFEST_FILE);
    let bytes = read_bounded(&manifest_path)?;
    ensure!(
        crate::util::sha256::sha256_hex(&bytes) == expected_sha256,
        "local campaign.ini differs from the admitted series identity"
    );
    let ini = IniFile::from_bytes_preserving_empty_values(&bytes)?;
    let manifest = parse_vmce_manifest(&ini)?;
    validate_vmce_manifest_maps(&series_dir, &manifest.maps)?;
    let (_, current_entry) = split_catalog_map(&current.map_file)?;
    let Some(target) = next_scenario_token(&ini, &current_entry)? else {
        return Ok(None);
    };
    ensure!(
        manifest.maps.iter().any(|map| same_map_name(map, &target)),
        "campaign NextScenario is absent from ScenarioCount"
    );
    let assets = state
        .process_assets
        .manager()
        .context("VMCE next stage requires active retail assets")?;
    let loaded = load_vmce_map_ini(&maps_root, &series_dir, directory, &target, assets)?;
    let (byte_length, source_sha256) = loaded
        .source
        .physical_identity()
        .map(|(length, digest)| (length, digest.to_owned()))
        .context("VMCE next stage requires a physical authored map")?;
    let effective_digest = crate::map::source::effective_network_map_digest(&loaded.ini, false);
    let display_name = loaded
        .ini
        .section("Basic")
        .map(|section| section.read_string("Name", "", 0x20))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| target.clone());
    let config = load_vmce_scenario(&series_dir, &target, &loaded.ini)?;
    let next = crate::app::match_bootstrap::NetworkVmceMapConfig::new(
        catalog_map_file(directory, &target),
        source_sha256.clone(),
        config,
    )?
    .with_series_identity(directory.to_owned(), expected_sha256.to_owned())?;
    let metadata = crate::app::match_runtime::session::NetworkMapMetadata {
        display_name,
        player_capacity: 8,
        official: false,
        byte_length: i32::try_from(byte_length).unwrap_or(i32::MAX),
        digest: String::new(),
        source_sha256,
        effective_digest: effective_digest.clone(),
    };
    Ok(Some(crate::app::match_bootstrap::VmceNextStage {
        map_file: catalog_map_file(directory, &target),
        config: next,
        metadata,
        local_map_source: loaded.source,
        effective_digest,
    }))
}

fn vmce_roots(state: &AppState) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    let game_config = state
        .platform
        .game_config
        .as_ref()
        .context("VMCE series requires profile paths")?;
    let maps_root = game_config.paths.settings_dir().join("Maps");
    let vmce_root = maps_root.join(VMCE_ROOT);
    Ok((maps_root, vmce_root))
}

/// Refresh validated campaign Entry maps in the shared scenario catalog.
/// Each direct child of `settings/Maps/VMCE` is one series campaign; its
/// `campaign.ini` and all manifest-relative maps live in that child.
pub(crate) fn refresh_vmce_entries(state: &mut AppState) {
    {
        let mut records = state.frontend.scenario_catalog.records_mut();
        records.retain(|record| {
            !matches!(
                &record.source,
                crate::map::skirmish_scenarios::SkirmishScenarioSource::VmceSeries(_)
            )
        });
    }
    let Some(game_config) = state.platform.game_config.as_ref() else {
        return;
    };
    let maps_root = game_config.paths.settings_dir().join("Maps");
    let vmce_root = maps_root.join(VMCE_ROOT);
    let Ok(canonical_root) = vmce_root.canonicalize() else {
        return;
    };
    let Some(assets) = state.process_assets.manager() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&canonical_root) else {
        return;
    };
    let mut series_entries: Vec<_> = entries.flatten().collect();
    series_entries.sort_by_key(|entry| entry.file_name());
    let mut discovered = Vec::new();
    for entry in series_entries {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Ok(series_dir) = entry.path().canonicalize() else {
            continue;
        };
        if !series_dir.starts_with(&canonical_root) {
            log::warn!(
                "ignoring VMCE series outside the VMCE root: {}",
                entry.path().display()
            );
            continue;
        }
        let series = entry.file_name().to_string_lossy().into_owned();
        let manifest_path = series_dir.join(VMCE_MANIFEST_FILE);
        let parsed = read_bounded(&manifest_path)
            .and_then(|bytes| {
                IniFile::from_bytes_preserving_empty_values(&bytes).map_err(anyhow::Error::from)
            })
            .and_then(|ini| {
                let manifest = parse_vmce_manifest(&ini)?;
                validate_vmce_manifest_maps(&series_dir, &manifest.maps)?;
                Ok(manifest)
            });
        let manifest = match parsed {
            Ok(manifest) => manifest,
            Err(error) => {
                log::warn!(
                    "ignoring invalid VMCE series {}: {error:#}",
                    manifest_path.display()
                );
                continue;
            }
        };
        let Ok(loaded) =
            load_vmce_map_ini(&maps_root, &series_dir, &series, &manifest.entry, assets)
        else {
            continue;
        };
        let display_name = resolve_series_display_name(state, &series_dir, &manifest.name, &series);
        log::debug!("VMCE series {series}: display name {display_name:?}");
        let mut record = crate::map::skirmish_scenarios::SkirmishScenarioRecord::concrete_from_ini(
            discovered.len(),
            crate::map::skirmish_scenarios::SkirmishScenarioSource::VmceSeries(series.clone()),
            &catalog_map_file(&series, &manifest.entry),
            &loaded.ini,
        );
        record.display_name = display_name;
        if load_vmce_scenario(&series_dir, &manifest.entry, &loaded.ini).is_err() {
            log::warn!(
                "ignoring VMCE series {} with invalid entry configuration",
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
    log::debug!("VMCE series discovery: {} record(s)", discovered.len());
    let mut records = state.frontend.scenario_catalog.records_mut();
    records.extend(discovered);
}

fn load_vmce_map_ini(
    maps_root: &Path,
    series_dir: &Path,
    series: &str,
    entry: &str,
    assets: &crate::assets::asset_manager::AssetManager,
) -> Result<LoadedMapIni> {
    crate::map::source::validate_network_map_name(entry)?;
    ensure!(
        !series.is_empty()
            && Path::new(series).components().count() == 1
            && Path::new(series)
                .components()
                .next()
                .is_some_and(|part| matches!(part, std::path::Component::Normal(_))),
        "invalid VMCE series directory"
    );
    let canonical_series = series_dir
        .canonicalize()
        .with_context(|| format!("VMCE series directory is missing: {}", series_dir.display()))?;
    let expected = canonical_series
        .join(entry)
        .canonicalize()
        .with_context(|| format!("VMCE series map is missing: {entry}"))?;
    ensure!(
        expected.starts_with(&canonical_series),
        "campaign map escapes the VMCE series directory"
    );
    let logical_name = catalog_map_file(series, entry);
    let loaded = load_network_map_ini(maps_root, &logical_name, assets)?;
    validate_vmce_source(&loaded.source, &expected)?;
    Ok(loaded)
}

fn validate_vmce_source(source: &LoadedMapSource, expected: &Path) -> Result<()> {
    let LoadedMapSource::Loose { path, .. } = source else {
        anyhow::bail!("VMCE series maps must resolve to loose files under Maps/VMCE/<series>")
    };
    let actual = path.canonicalize()?;
    ensure!(
        actual == expected,
        "VMCE map source does not match its series manifest path"
    );
    Ok(())
}

pub(crate) fn load_vmce_scenario(
    series_dir: &Path,
    map_file: &str,
    map_ini: &IniFile,
) -> Result<crate::map::vmce_config::VmceMapConfig> {
    crate::map::source::validate_network_map_name(map_file)?;
    let series_dir = series_dir
        .canonicalize()
        .with_context(|| format!("VMCE series directory is missing: {}", series_dir.display()))?;
    let manifest_path = series_dir.join(VMCE_MANIFEST_FILE);
    let bytes = read_bounded(&manifest_path)
        .with_context(|| format!("VMCE campaign.ini is missing: {}", manifest_path.display()))?;
    let ini = IniFile::from_bytes_preserving_empty_values(&bytes).map_err(anyhow::Error::from)?;
    let manifest = parse_vmce_manifest(&ini)?;
    validate_vmce_manifest_maps(&series_dir, &manifest.maps)?;
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
    let mut config = crate::map::vmce_config::VmceMapConfig::from_ini(map_ini, &external)?;
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

struct VmceManifest {
    /// Optional CSF label for the series display name. It is resolved from
    /// the series' own `ra2md.csf` first, then the base game table.
    name: Option<String>,
    entry: String,
    maps: Vec<String>,
}

fn parse_vmce_manifest(ini: &IniFile) -> Result<VmceManifest> {
    ensure!(
        !ini.has_duplicate_sections() && !ini.has_duplicate_keys(),
        "campaign INI contains repeated physical sections or keys"
    );
    let section = ini
        .section("Campaign")
        .context("campaign INI requires [Campaign]")?;
    let name = section.read_string("Name", "", 1024).trim().to_owned();
    let name = (!name.is_empty()).then_some(name);
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
    Ok(VmceManifest { name, entry, maps })
}

fn validate_vmce_manifest_maps(series_dir: &Path, maps: &[String]) -> Result<()> {
    for map in maps {
        let relative = Path::new(&map.replace('\\', "/")).to_path_buf();
        let path = series_dir.join(relative);
        let canonical = path
            .canonicalize()
            .with_context(|| format!("campaign map is missing: {map}"))?;
        ensure!(
            canonical.starts_with(series_dir),
            "campaign map escapes the VMCE series directory: {map}"
        );
        ensure!(canonical.is_file(), "campaign map is not a file: {map}");
    }
    Ok(())
}

fn series_root(vmce_root: &Path, series: &str) -> Result<PathBuf> {
    ensure!(
        !series.is_empty()
            && Path::new(series).components().count() == 1
            && Path::new(series)
                .components()
                .next()
                .is_some_and(|part| matches!(part, std::path::Component::Normal(_))),
        "invalid VMCE series directory"
    );
    let root = vmce_root.canonicalize().context("VMCE root is missing")?;
    let dir = root
        .join(series)
        .canonicalize()
        .with_context(|| format!("VMCE series directory is missing: {series}"))?;
    ensure!(
        dir.starts_with(&root),
        "VMCE series directory escapes the VMCE root"
    );
    Ok(dir)
}

fn catalog_map_file(series: &str, entry: &str) -> String {
    format!("{VMCE_ROOT}/{series}/{}", entry.replace('\\', "/"))
}

fn split_catalog_map(map_file: &str) -> Result<(String, String)> {
    let relative = map_path_from_catalog(map_file)?;
    let relative = relative.replace('\\', "/");
    let (series, entry) = relative
        .split_once('/')
        .context("VMCE map must name a series directory and a relative map")?;
    ensure!(
        !series.is_empty()
            && !entry.is_empty()
            && Path::new(series).components().count() == 1
            && Path::new(series)
                .components()
                .next()
                .is_some_and(|part| matches!(part, std::path::Component::Normal(_))),
        "invalid VMCE series directory"
    );
    crate::map::source::validate_network_map_name(entry)?;
    Ok((series.to_owned(), entry.to_owned()))
}

fn resolve_series_display_name(
    state: &AppState,
    series_dir: &Path,
    label: &Option<String>,
    fallback: &str,
) -> String {
    let Some(label) = label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
    else {
        return fallback.to_owned();
    };
    let series_csf = std::fs::read(series_dir.join(VMCE_SERIES_CSF))
        .ok()
        .and_then(|bytes| match CsfFile::from_bytes(&bytes) {
            Ok(csf) => Some(csf),
            Err(error) => {
                log::warn!(
                    "ignoring invalid VMCE series CSF {}: {error}",
                    series_dir.join(VMCE_SERIES_CSF).display()
                );
                None
            }
        });
    resolve_csf_display_name(
        series_csf.as_ref(),
        state.process_assets.csf.as_ref(),
        label,
        fallback,
    )
}

fn resolve_csf_display_name(
    series_csf: Option<&CsfFile>,
    base_csf: Option<&CsfFile>,
    label: &str,
    fallback: &str,
) -> String {
    let text = series_csf
        .and_then(|csf| csf.get(label))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .or_else(|| {
            base_csf
                .and_then(|csf| csf.get(label))
                .map(str::trim)
                .filter(|text| !text.is_empty())
        });
    match text {
        Some(text) => text.to_owned(),
        None => {
            log::warn!(
                "VMCE series {fallback} name label {label:?} is missing from its CSF and the base CSF"
            );
            fallback.to_owned()
        }
    }
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
    let prefix = format!("{VMCE_ROOT}/");
    if normalized
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(&prefix))
    {
        Ok(normalized[prefix.len()..].to_owned())
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
            "VMCE series INI exceeds 64 KiB"
        );
        Ok(bytes)
    }
    #[cfg(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown"))]
    {
        let bytes = crate::util::file_system::read_shared(path)?;
        ensure!(
            bytes.len() <= MAX_CAMPAIGN_BYTES,
            "VMCE series INI exceeds 64 KiB"
        );
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmce_catalog_map_keeps_the_series_and_entry_separate() {
        let (series, entry) = split_catalog_map("VMCE/LDA/all01umd.map").unwrap();
        assert_eq!(series, "LDA");
        assert_eq!(entry, "all01umd.map");
        let (series, entry) = split_catalog_map("VMCE/林德拉克计划/Allied/01.map").unwrap();
        assert_eq!(series, "林德拉克计划");
        assert_eq!(entry, "Allied/01.map");
        assert!(split_catalog_map("VMCE/no-entry.map").is_err());
    }

    #[test]
    fn vmce_series_name_prefers_its_own_csf_then_the_base_table_then_the_directory() {
        let series = crate::assets::csf_file::test_csf(&[("LDA:Title", "林德拉克计划")]);
        let base = crate::assets::csf_file::test_csf(&[("BASE:Title", "Base title")]);
        assert_eq!(
            resolve_csf_display_name(Some(&series), Some(&base), "LDA:Title", "folder"),
            "林德拉克计划"
        );
        assert_eq!(
            resolve_csf_display_name(Some(&series), Some(&base), "BASE:Title", "folder"),
            "Base title"
        );
        assert_eq!(
            resolve_csf_display_name(Some(&series), Some(&base), "MISSING:Title", "folder"),
            "folder"
        );
    }

    #[test]
    fn campaign_manifest_requires_entry_and_rejects_duplicate_or_escaping_paths() {
        let valid = IniFile::from_str(
            "[Campaign]\nName=Name:AD03\nEntry=Allied/01.map\nScenarioCount=2\nScenario1=Allied/01.map\nScenario2=Soviet/02.map\n",
        );
        let manifest = parse_vmce_manifest(&valid).unwrap();
        assert_eq!(manifest.name.as_deref(), Some("Name:AD03"));
        assert_eq!(manifest.entry, "Allied/01.map");
        assert_eq!(manifest.maps.len(), 2);

        let duplicate = IniFile::from_str(
            "[Campaign]\nEntry=01.map\nScenarioCount=2\nScenario1=01.map\nScenario2=01.MAP\n",
        );
        assert!(parse_vmce_manifest(&duplicate).is_err());
        let escaping = IniFile::from_str(
            "[Campaign]\nEntry=../01.map\nScenarioCount=1\nScenario1=../01.map\n",
        );
        assert!(parse_vmce_manifest(&escaping).is_err());
        let duplicate_section = IniFile::from_str(
            "[Campaign]\nEntry=01.map\nScenarioCount=1\nScenario1=01.map\n[Campaign]\nScenarioCount=1\n",
        );
        assert!(parse_vmce_manifest(&duplicate_section).is_err());
    }

    #[test]
    fn campaign_scenario_projects_explicit_roles_and_controls_without_map_fallback() {
        let directory =
            std::env::temp_dir().join(format!("vera-campaign-campaign-{}", std::process::id()));
        let series_dir = directory.join("campaign-a");
        std::fs::create_dir_all(series_dir.join("Allied")).unwrap();
        std::fs::create_dir_all(series_dir.join("Soviet")).unwrap();
        std::fs::write(series_dir.join("Allied/01.map"), b"map").unwrap();
        std::fs::write(series_dir.join("Soviet/02.map"), b"map").unwrap();
        std::fs::write(
            series_dir.join(VMCE_MANIFEST_FILE),
            b"[Campaign]\nEntry=Allied/01.map\nScenarioCount=2\nScenario1=Allied/01.map\nScenario2=Soviet/02.map\n[Scenario:Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\nNextScenario=Soviet/02.map\n[Scenario:Allied/01.map:Neutral]\nPlayer1Control=yes\nPlayer2Control=no\n",
        ).unwrap();
        let map = IniFile::from_str(
            "[Basic]\nPlayer=Legacy\nPlayer1=OldA\nPlayer2=OldB\nPlayer3=OldC\n[Houses]\n0=Alpha\n1=Beta\n2=Neutral\n[Alpha]\n[Beta]\n[Neutral]\nPlayerControl=yes\n",
        );
        let config = load_vmce_scenario(&series_dir, "Allied/01.map", &map).unwrap();
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

        let manifest_path = series_dir.join(VMCE_MANIFEST_FILE);
        std::fs::write(
            &manifest_path,
            b"[Campaign]\nEntry=Allied/01.map\nScenarioCount=2\nScenario1=Allied/01.map\nScenario2=Soviet/02.map\n[Scenario:Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\nPlayer3=Neutral\n",
        )
        .unwrap();
        assert!(load_vmce_scenario(&series_dir, "Allied/01.map", &map).is_err());
        std::fs::write(
            &manifest_path,
            b"[Campaign]\nEntry=Allied/01.map\nScenarioCount=2\nScenario1=Allied/01.map\nScenario2=Soviet/02.map\n[Scenario:Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\nNextScenario=Soviet/02.map\nEndOfGame=maybe\n",
        )
        .unwrap();
        assert!(load_vmce_scenario(&series_dir, "Allied/01.map", &map).is_err());
        std::fs::write(
            &manifest_path,
            b"[Campaign]\nEntry=Allied/01.map\nScenarioCount=2\nScenario1=Allied/01.map\nScenario2=Soviet/02.map\n[Scenario:Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\nNextScenario=missing.map\n",
        )
        .unwrap();
        assert!(load_vmce_scenario(&series_dir, "Allied/01.map", &map).is_err());
        std::fs::write(
            &manifest_path,
            b"[Campaign]\nEntry=Allied/01.map\nScenarioCount=2\nScenario1=Allied/01.map\nScenario2=Soviet/02.map\n[Scenario:Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\nNextScenario=Soviet/02.map\nEndOfGame=maybe\n",
        )
        .unwrap();
        assert!(load_vmce_scenario(&series_dir, "Allied/01.map", &map).is_err());
        std::fs::write(
            &manifest_path,
            b"[Campaign]\nEntry=Allied/01.map\nScenarioCount=2\nScenario1=Allied/01.map\nScenario2=Soviet/02.map\n[Scenario:Allied/01.map]\nPlayerCount=2\nPlayer1=Alpha\nPlayer2=Beta\n[Scenario:Allied/01.map:Ghost]\nPlayer1Control=yes\n",
        )
        .unwrap();
        assert!(load_vmce_scenario(&series_dir, "Allied/01.map", &map).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn campaign_manifest_requires_every_listed_map_to_exist_inside_root() {
        let directory =
            std::env::temp_dir().join(format!("vera-campaign-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("present.map"), b"map").unwrap();
        let root = directory.canonicalize().unwrap();
        assert!(validate_vmce_manifest_maps(&root, &["present.map".to_owned()]).is_ok());
        assert!(validate_vmce_manifest_maps(&root, &["missing.map".to_owned()]).is_err());
        assert!(validate_vmce_manifest_maps(&root, &["../outside.map".to_owned()]).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn campaign_loader_rejects_retail_or_mix_source_for_manifest_path() {
        let directory =
            std::env::temp_dir().join(format!("vera-campaign-source-{}", std::process::id()));
        let campaign = directory.join("VMCE");
        std::fs::create_dir_all(&campaign).unwrap();
        let expected = campaign.join("entry.map");
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
        assert!(validate_vmce_source(&source(expected.clone()), &expected).is_ok());
        assert!(validate_vmce_source(&source(impostor), &expected).is_err());
        let archive = LoadedMapSource::Mix {
            logical_name: "VMCE/entry.map".to_owned(),
            source_archive: "ra2.mix".to_owned(),
            entry_id: 1,
            payload_len: 1,
            source_sha256: "a".repeat(64),
        };
        assert!(validate_vmce_source(&archive, &expected).is_err());
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
