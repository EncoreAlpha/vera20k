//! The selected LAN host's external VERA Coop map settings.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

use crate::app::AppState;
use crate::app::match_bootstrap::NetworkVeraCoopMapConfig;
use crate::map::source::{LoadedMapSource, load_network_map_ini};
use crate::rules::ini_parser::IniFile;

const MAX_SUPPLEMENTAL_BYTES: usize = 4 * 1024;

/// Resolve the map plus its optional `<complete map filename>.coop.ini`.
/// A missing sidecar is a valid empty overlay; malformed or oversized files
/// reject selection so the host cannot advertise a partial configuration.
pub(crate) fn resolve_host_vera_coop_map_config(
    state: &AppState,
    map_file: &str,
) -> Result<NetworkVeraCoopMapConfig> {
    crate::map::source::validate_network_map_name(map_file)?;
    let root = &state
        .platform
        .game_config
        .as_ref()
        .context("VERA Coop map config requires the retail root")?
        .paths
        .ra2_dir;
    let assets = state
        .process_assets
        .manager()
        .context("VERA Coop map config requires active retail assets")?;
    let loaded = load_network_map_ini(root, map_file, assets)?;
    let source_sha256 = loaded
        .source
        .physical_identity()
        .map(|(_, digest)| digest.to_owned())
        .context("VERA Coop requires a physical authored map")?;
    let config = load_supplemental_config(root, map_file, &loaded.source, &loaded.ini)?;
    NetworkVeraCoopMapConfig::new(map_file.to_owned(), source_sha256, config)
}

fn load_supplemental_config(
    root: &Path,
    map_file: &str,
    source: &LoadedMapSource,
    map_ini: &IniFile,
) -> Result<crate::map::coop_config::VeraCoopMapConfig> {
    let candidates = match source {
        LoadedMapSource::Loose { path, .. } => vec![sidecar_for_path(path)?],
        LoadedMapSource::Mix { .. } => {
            let name = format!("{map_file}.coop.ini");
            let mut paths = vec![root.join(&name)];
            if let Some(downloaded) = crate::map::source::downloaded_maps_dir() {
                paths.push(downloaded.join(name));
            }
            paths
        }
        LoadedMapSource::Generated { .. } => {
            anyhow::bail!("VERA Coop sidecars require an authored map")
        }
        LoadedMapSource::LegacyFallback { .. } => {
            anyhow::bail!("VERA Coop sidecars require a verified map source")
        }
    };
    let mut found = Vec::new();
    for path in candidates {
        if crate::util::file_system::try_exists(&path)? {
            found.push(path);
        }
    }
    ensure!(
        found.len() <= 1,
        "multiple VERA Coop sidecars match {map_file:?}"
    );
    let Some(path) = found.pop() else {
        return Ok(crate::map::coop_config::VeraCoopMapConfig::default());
    };
    let bytes = read_bounded(&path)?;
    crate::map::coop_config::VeraCoopMapConfig::from_bytes(map_ini, &bytes)
        .with_context(|| format!("invalid VERA Coop sidecar {}", path.display()))
}

fn sidecar_for_path(map_path: &Path) -> Result<PathBuf> {
    let name = map_path
        .file_name()
        .context("selected map path has no filename")?
        .to_string_lossy();
    let sidecar_name = format!("{name}.coop.ini");
    Ok(map_path.with_file_name(sidecar_name))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    #[cfg(not(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown")))]
    {
        use std::io::Read;
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take((MAX_SUPPLEMENTAL_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= MAX_SUPPLEMENTAL_BYTES,
            "VERA Coop sidecar exceeds 4 KiB"
        );
        Ok(bytes)
    }
    #[cfg(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown"))]
    {
        let bytes = crate::util::file_system::read_shared(path)?;
        ensure!(
            bytes.len() <= MAX_SUPPLEMENTAL_BYTES,
            "VERA Coop sidecar exceeds 4 KiB"
        );
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loose_map_sidecar_loader_preserves_explicit_empty_and_unterminated_last_value() {
        let directory =
            std::env::temp_dir().join(format!("vera-coop-sidecar-loader-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let map_path = directory.join("authored.map");
        let sidecar_path = directory.join("authored.map.coop.ini");
        std::fs::write(&sidecar_path, b"[Basic]\nPlayer1=\nPlayer2=Beta").unwrap();

        let map_ini = IniFile::from_str(
            "[Basic]\nPlayer=Beta\nPlayer1=Alpha\n[Houses]\n0=Alpha\n1=Beta\n[Alpha]\n[Beta]\n",
        );
        let source = LoadedMapSource::Loose {
            path: map_path,
            payload_len: 1,
            source_sha256: "a".repeat(64),
            cache_lease: Default::default(),
        };
        let config =
            load_supplemental_config(&directory, "authored.map", &source, &map_ini).unwrap();
        let projected = config.apply_to_map(&map_ini).unwrap();
        let basic = crate::map::basic::parse_basic_section(&projected);
        assert_eq!(
            basic.campaign_players.players[0].owner.as_deref(),
            Some("Beta")
        );
        assert_eq!(
            basic.campaign_players.players[1].owner.as_deref(),
            Some("Beta")
        );

        std::fs::remove_file(sidecar_path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
