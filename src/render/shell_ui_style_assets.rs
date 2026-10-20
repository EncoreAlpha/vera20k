//! Explicit source selection for shell assets with a confirmed RA2/YR variant.
//!
//! Shared and YR-only resources continue through `AssetManager`'s ordinary
//! lookup. A RA2 style choice names its exact archive and member so a missing
//! RA2 copy is diagnosable instead of silently resolving the active YR asset.

use crate::assets::asset_manager::AssetManager;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ShellUiAssetSource {
    Active,
    Archive(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShellUiAssetLocator<'name> {
    pub(crate) member: &'name str,
    pub(crate) source: ShellUiAssetSource,
}

impl<'name> ShellUiAssetLocator<'name> {
    pub(crate) const fn active(member: &'name str) -> Self {
        Self {
            member,
            source: ShellUiAssetSource::Active,
        }
    }

    pub(crate) const fn archive(member: &'name str, archive: &'static str) -> Self {
        Self {
            member,
            source: ShellUiAssetSource::Archive(archive),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShellUiAssetPair<'name> {
    pub(crate) ra2: ShellUiAssetLocator<'name>,
    pub(crate) yr: ShellUiAssetLocator<'name>,
}

impl<'name> ShellUiAssetPair<'name> {
    pub(crate) const fn new(
        ra2: ShellUiAssetLocator<'name>,
        yr: ShellUiAssetLocator<'name>,
    ) -> Self {
        Self { ra2, yr }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShellUiAssetRef<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) source: &'a str,
}

pub(crate) struct ShellUiAssetResolver<'a> {
    assets: &'a AssetManager,
    yr_original_style: bool,
}

impl<'a> ShellUiAssetResolver<'a> {
    pub(crate) const fn new(assets: &'a AssetManager, yr_original_style: bool) -> Self {
        Self {
            assets,
            yr_original_style,
        }
    }

    pub(crate) fn resolve(
        &self,
        pair: ShellUiAssetPair<'_>,
    ) -> anyhow::Result<ShellUiAssetRef<'a>> {
        let locator = if self.yr_original_style {
            pair.yr
        } else {
            pair.ra2
        };
        match locator.source {
            ShellUiAssetSource::Active => {
                let (bytes, source) =
                    self.assets
                        .get_with_source_ref(locator.member)
                        .ok_or_else(|| {
                            anyhow::anyhow!("missing active shell asset {}", locator.member)
                        })?;
                Ok(ShellUiAssetRef { bytes, source })
            }
            ShellUiAssetSource::Archive(archive_name) => {
                let (source, archive) =
                    self.assets
                        .archive_with_source(archive_name)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "missing shell style archive {archive_name} for {}",
                                locator.member
                            )
                        })?;
                let bytes = archive.get_by_name(locator.member).ok_or_else(|| {
                    anyhow::anyhow!("missing shell style member {} in {source}", locator.member)
                })?;
                Ok(ShellUiAssetRef { bytes, source })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::asset_manager::MediaArchiveMode;

    #[test]
    #[ignore = "requires RA2_DIR with installed retail RA2/YR assets"]
    fn source_selection_uses_exact_ra2_archive_and_active_yr_winner() {
        let retail_dir = std::path::PathBuf::from(
            std::env::var_os("RA2_DIR").expect("set RA2_DIR to the retail RA2/YR install"),
        );
        let mut assets = AssetManager::new(&retail_dir, MediaArchiveMode::STOCK_DIGITAL)
            .expect("load retail RA2/YR assets");
        assert!(
            assets
                .register_neutral_archives()
                .expect("register production neutral shell archives")
        );
        let coop_background = ShellUiAssetPair::new(
            ShellUiAssetLocator::archive("MNSCRNL.SHP", "ra2.mix -> neutral.mix"),
            ShellUiAssetLocator::active("MnScrnLCoopGameSetup.shp"),
        );

        let ra2 = ShellUiAssetResolver::new(&assets, false)
            .resolve(coop_background)
            .expect("exact Red Alert 2 background");
        assert_eq!(ra2.source, "ra2.mix -> neutral.mix");

        let yr = ShellUiAssetResolver::new(&assets, true)
            .resolve(coop_background)
            .expect("active Yuri's Revenge background");
        assert_eq!(yr.source, "ra2md.mix -> ntrlmd.mix");
        assert_ne!(ra2.bytes, yr.bytes);
    }

    #[test]
    #[ignore = "requires RA2_DIR with installed retail RA2/YR assets"]
    fn shell_skin_pairs_resolve_the_expected_retail_members() {
        let retail_dir = std::path::PathBuf::from(
            std::env::var_os("RA2_DIR").expect("set RA2_DIR to the retail RA2/YR install"),
        );
        let mut assets = AssetManager::new(&retail_dir, MediaArchiveMode::STOCK_DIGITAL)
            .expect("load retail RA2/YR assets");
        assert!(
            assets
                .register_neutral_archives()
                .expect("register production neutral shell archives")
        );
        let ra2 = ShellUiAssetResolver::new(&assets, false);
        let yr = ShellUiAssetResolver::new(&assets, true);

        for (pair, ra2_source, yr_source, should_differ) in [
            (
                ShellUiAssetPair::new(
                    ShellUiAssetLocator::archive("MNSCRNS.SHP", "ra2.mix -> neutral.mix"),
                    ShellUiAssetLocator::active("MNSCRNS.SHP"),
                ),
                "ra2.mix -> neutral.mix",
                "ra2.mix -> neutral.mix",
                false,
            ),
            (
                ShellUiAssetPair::new(
                    ShellUiAssetLocator::archive("MNSCRNL.SHP", "ra2.mix -> neutral.mix"),
                    ShellUiAssetLocator::active("MnScrnLCustomizeBattle.shp"),
                ),
                "ra2.mix -> neutral.mix",
                "ra2md.mix -> ntrlmd.mix",
                true,
            ),
            (
                ShellUiAssetPair::new(
                    ShellUiAssetLocator::archive("SHELL.PAL", "ra2.mix -> cache.mix"),
                    ShellUiAssetLocator::active("MnScrnLCoopGameSetup.PAL"),
                ),
                "ra2.mix -> cache.mix",
                "ra2md.mix -> ntrlmd.mix",
                true,
            ),
            (
                ShellUiAssetPair::new(
                    ShellUiAssetLocator::archive("PUDLGBGN.SHP", "ra2.mix -> local.mix"),
                    ShellUiAssetLocator::active("PUDLGBGN.SHP"),
                ),
                "ra2.mix -> local.mix",
                "expandmd01.mix",
                true,
            ),
            (
                ShellUiAssetPair::new(
                    ShellUiAssetLocator::archive("MNBTTN.SHP", "ra2.mix -> local.mix"),
                    ShellUiAssetLocator::active("MNBTTN.SHP"),
                ),
                "ra2.mix -> local.mix",
                "ra2md.mix -> localmd.mix",
                true,
            ),
            (
                ShellUiAssetPair::new(
                    ShellUiAssetLocator::archive("cue_i.pcx", "ra2.mix -> local.mix"),
                    ShellUiAssetLocator::active("cue_i.pcx"),
                ),
                "ra2.mix -> local.mix",
                "ra2md.mix -> localmd.mix",
                true,
            ),
        ] {
            let ra2_asset = ra2.resolve(pair).expect("resolve exact RA2 source");
            let yr_asset = yr.resolve(pair).expect("resolve active YR source");
            assert_eq!(ra2_asset.source, ra2_source);
            assert_eq!(yr_asset.source, yr_source);
            assert_eq!(ra2_asset.bytes != yr_asset.bytes, should_differ);
        }

        // These mode-2 resources have one retail member and remain shared
        // through ordinary lookup in either shell style.
        for member in ["PUDLGBGA.SHP", "PUDLGBGS.SHP", "DIALOG.PAL"] {
            let (bytes, source) = assets
                .get_with_source_ref(member)
                .unwrap_or_else(|| panic!("missing shared shell asset {member}"));
            assert_eq!(source, "ra2.mix -> local.mix", "{member}");
            assert!(!bytes.is_empty(), "{member}");
        }
    }
}
