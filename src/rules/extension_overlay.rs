//! The 32-player extension's rules layer — data that is **not** retail.
//!
//! # Why this layer exists
//!
//! `AGENTS.md` requires that gamemd behavior comes from the original INI keys,
//! and that any departure be labelled rather than presented as parity. The
//! 32-player extension needs more house colors than stock `[Colors]` provides
//! (21 entries, of which 18 are usable player colors), and ModEnc documents that
//! the section "can be freely added to... as long as they have unique names".
//! That makes appending entries a supported extension point — but the appended
//! entries are extension data, not retail data, so they must not be written into
//! the player's `rulesmd.ini`.
//!
//! This module is that separation made explicit in code: the extra schemes live
//! here as constants, are pushed as a distinct [`RulesLayerKind::Extension`],
//! and therefore never mix with a retail or map-authored source.
//!
//! # Naming
//!
//! The module is called "extension", not after the project: the project is a
//! community reimplementation (`README.md`: "VERA20k is made by gamers, for
//! gamers, and gamers have the final say"), so a project-named layer would read
//! as ownership of the data rather than provenance of it. What matters downstream
//! is only that the layer is distinguishable from retail, which the enum variant
//! and this name both convey.
//!
//! # Ordering is load-bearing
//!
//! `parse_color_schemes` keeps entries in **section declaration order**, and
//! `PRIORITY_TO_SCHEME_INDEX` addresses `[Colors]` entries by that order. The
//! stock layer must therefore always be applied first, and this layer last, so
//! that the indices in `EXTENSION_COLOR_SCHEMES` line up with what the parser sees.
//! `NativeRulesProcessOwner::scenario_layers` pushes this layer last for that
//! reason.
//!
//! # Merge semantics
//!
//! `NativeRulesProcessOwner::allocate_colors` is keep-first: an entry already
//! present wins and a duplicate key here is dropped. The names below are
//! deliberately unlikely to collide with a mod's own `[Colors]` additions — a
//! collision would silently discard our entry rather than theirs, which is the
//! safe direction but still worth avoiding.
//!
//! # Values
//!
//! Derived by `todo/color32.py`, which implements gamemd's 6-sextant
//! integer HSV->RGB (`color_scheme.rs::hsv_to_rgb`) to report perceptual
//! collisions. gamemd buckets hue into six sextants (`h * 6 / 255`), so 32
//! player colors cannot all be separated by hue alone — the scheme spaces the
//! new hues within each sextant and rotates saturation/value pairs so
//! neighbors never repeat.
//!
//! Native slots 0..=8 keep the stock table unchanged and are *not* repeated
//! here; this list covers only the extension, entries 21..=35 of the merged
//! `[Colors]` section.
//!
//! Entry names are prefixed to stay collision-resistant: `allocate_colors` is
//! keep-first, so a name a mod also added would silently drop *this* entry and
//! shift every later index by one. Stock names like `Purple2`/`Purple3` invite
//! exactly that (`Purple4`), so the extension uses its own namespace.

use crate::rules::ini_parser::IniFile;

/// `[Colors]` entries appended after the 21 stock ones.
///
/// H,S,V bytes, gamemd's scale (0..=255 each), in declaration order.
/// Entry 21 is the first one addressable by the extension half of
/// `PRIORITY_TO_SCHEME_INDEX` (which stores `entry * 2 + 1`).
pub const EXTENSION_COLOR_SCHEMES: [(&str, [u8; 3]); 15] = [
    ("ExtColor17", [10, 245, 255]),
    ("ExtColor18", [21, 255, 210]),
    ("ExtColor19", [32, 190, 250]),
    ("ExtColor20", [53, 245, 255]),
    ("ExtColor21", [63, 255, 210]),
    ("ExtColor22", [74, 190, 250]),
    ("ExtColor23", [95, 245, 255]),
    ("ExtColor24", [106, 255, 210]),
    ("ExtColor25", [117, 190, 250]),
    ("ExtColor26", [142, 245, 255]),
    ("ExtColor27", [155, 255, 210]),
    ("ExtColor28", [184, 245, 255]),
    ("ExtColor29", [198, 255, 210]),
    ("ExtColor30", [227, 245, 255]),
    ("ExtColor31", [240, 255, 210]),
];

/// The `[Colors]` entries as INI text, ready for [`IniFile::from_str`].
///
/// Built once and cached: `scenario_layers` pushes this on every scenario load,
/// and parsing the same text each time is pure waste.
///
/// Kept as a plain string rather than assembled key by key so the layer's
/// contents stay readable in one place and diffable in review.
pub fn extension_colors_ini() -> IniFile {
    static CACHE: std::sync::OnceLock<IniFile> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let mut text = String::with_capacity(512);
            text.push_str(
                "; VERA20k 32-player extension -- NOT retail data.\n\
                 ; Appended after the stock [Colors] entries; see src/rules/extension_overlay.rs.\n\
                 ; Stock [Colors] entries 0..=20 are untouched.\n\
                 [Colors]\n",
            );
            for (name, [h, s, v]) in EXTENSION_COLOR_SCHEMES {
                text.push_str(&format!("{name}={h},{s},{v}\n"));
            }
            IniFile::from_str(&text)
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::color_scheme::{parse_color_schemes, PRIORITY_TO_SCHEME_INDEX};

    /// Every index the extension half of `PRIORITY_TO_SCHEME_INDEX` addresses
    /// must exist once stock and overlay are merged. Entry index = scheme / 2,
    /// and stock `[Colors]` has 21 entries, so the overlay must cover 21..=35.
    #[test]
    fn every_extension_slot_resolves_to_an_overlay_entry() {
        let stock_count = 21_usize;
        let mut merged = String::from("[Colors]\n");
        for (index, name) in [
            "LightGold", "Gold", "LightGrey", "Grey", "Red", "DarkRed", "Orange",
            "Magenta", "Purple", "LightBlue", "DarkBlue", "NeonBlue", "DarkSky",
            "Green", "DarkGreen", "NeonGreen", "Yellow", "Purple2", "Purple3",
            "AlliedLoad", "SovietLoad",
        ]
        .into_iter()
        .enumerate()
        {
            merged.push_str(&format!("{name}={},{},{}\n", index * 7 % 256, 200, 240));
        }
        let overlay = extension_colors_ini();
        if let Some(section) = overlay.section("Colors") {
            for (key, value) in section.raw_entries() {
                merged.push_str(&format!("{key}={value}\n"));
            }
        }

        let schemes = parse_color_schemes(&IniFile::from_str(&merged));
        assert_eq!(
            schemes.len(),
            stock_count + EXTENSION_COLOR_SCHEMES.len(),
            "merged [Colors] must be stock entries plus the overlay"
        );

        for (priority, &scheme) in PRIORITY_TO_SCHEME_INDEX.iter().enumerate() {
            let entry = scheme / 2;
            assert!(
                entry < schemes.len(),
                "priority {priority} needs [Colors] entry {entry}, only {} exist",
                schemes.len()
            );
        }
    }

    /// The stock entries the extension reuses must not be the three that are
    /// unsafe as player colors: NeonGreen (15) hides superweapon timers and
    /// power bars, AlliedLoad/SovietLoad (19/20) are loading-screen only.
    #[test]
    fn extension_reuses_no_unsafe_stock_entries() {
        for priority in 9..=16 {
            let entry = PRIORITY_TO_SCHEME_INDEX[priority] / 2;
            assert!(
                !matches!(entry, 15 | 19 | 20),
                "priority {priority} reuses unsafe stock entry {entry}"
            );
        }
    }

    /// The three stock entries that must never become player colors must not
    /// appear anywhere in the priority table.
    #[test]
    fn priority_table_never_targets_the_unsafe_stock_entries() {
        for (priority, &scheme) in PRIORITY_TO_SCHEME_INDEX.iter().enumerate() {
            let entry = scheme / 2;
            assert_ne!(entry, 15, "priority {priority} targets NeonGreen");
            assert_ne!(entry, 19, "priority {priority} targets AlliedLoad");
            assert_ne!(entry, 20, "priority {priority} targets SovietLoad");
        }
    }

    /// Stock parity: the first nine priorities must keep their native values,
    /// since those decide the stock 8-player colors.
    #[test]
    fn stock_priorities_keep_the_native_table() {
        assert_eq!(
            &PRIORITY_TO_SCHEME_INDEX[..9],
            &[3, 11, 21, 29, 13, 25, 17, 15, 5],
            "slots 0..=8 are native and must not move"
        );
    }

    /// The table covers 32 player slots plus the trailing default. A short table
    /// silently makes `scheme_index_for_priority` fall through to pass-through,
    /// which resolves to a different color than the one assigned here.
    #[test]
    fn priority_table_covers_thirty_two_slots_plus_the_default() {
        assert_eq!(PRIORITY_TO_SCHEME_INDEX.len(), 33);
        // Slot 32 is the random/observer default and repeats slot 8.
        assert_eq!(
            PRIORITY_TO_SCHEME_INDEX[32], PRIORITY_TO_SCHEME_INDEX[8],
            "the trailing default must match the stock default (LightGrey)"
        );
    }

    /// Overlay entries must be unique and case-insensitively distinct from each
    /// other: `allocate_colors` is keep-first, so a duplicate would be silently
    /// dropped and shift every later entry's index.
    #[test]
    fn overlay_names_are_unique() {
        let mut names: Vec<String> = EXTENSION_COLOR_SCHEMES
            .iter()
            .map(|(name, _)| name.to_ascii_uppercase())
            .collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate [Colors] name in the overlay");
    }

    /// Names stay in a neutral namespace: they must not carry the project name.
    ///
    /// The project is a community reimplementation and these entries are data
    /// *added to* it, so a project-branded name reads as ownership of the data
    /// rather than provenance of it. It also keeps the anti-collision property
    /// that motivated the prefix without claiming a name anyone could.
    #[test]
    fn overlay_names_do_not_brand_the_project() {
        for (name, _) in EXTENSION_COLOR_SCHEMES {
            let lowered = name.to_ascii_lowercase();
            assert!(
                !lowered.contains("vera"),
                "{name} carries the project name; extension entries must stay neutral"
            );
        }
    }
}