//! VERA Coop controls on the existing network setup page geometry.
//!
//! The original page owns the chrome anchors. Only the player columns are
//! rearranged: a map role already determines its House and country.

use crate::ui::multiplayer_setup::{GAME_OPTIONS, MultiplayerSetupLayout, SetupDialog};
use crate::ui::shell::geom::RectPx;

pub(crate) fn apply(
    layout: &mut MultiplayerSetupLayout,
    dialog: SetupDialog,
    row_count: usize,
    screen_width: i32,
    screen_height: i32,
) {
    // Use the same responsive mapping and resource anchors as the original
    // network setup, including its title, preview, labels and sidebar buttons.
    let native = MultiplayerSetupLayout::new(
        SetupDialog {
            modern: false,
            ..dialog
        },
        screen_width,
        screen_height,
        true,
    );
    let scale = native.page.scale;
    layout.page.scale = scale;
    layout.page.title = native.page.title;
    layout.page.status_help = native.page.status_help;
    layout.map_preview = native.map_preview;
    layout.no_preview = native.no_preview;
    layout.mode_label = native.mode_label;
    layout.map_label = native.map_label;
    layout.chat = native.chat;
    layout.chat_log = native.chat_log;
    layout.emote = native.emote;
    layout.acceptance = native.acceptance;
    layout.ping_icons = native.ping_icons;
    layout.cooperative_description = native.cooperative_description;
    layout
        .page
        .buttons
        .retain(|button| button.id != GAME_OPTIONS);
    for button in &mut layout.page.buttons {
        if let Some(rect) = native.page.button_rect(button.id) {
            button.rect = rect;
        }
    }

    let rect = |x, y, w, h| scale.scale_rect(RectPx::new(x, y, w, h));
    layout.labels = vec![
        ("GUI:Players", rect(74, 11, 180, 17)),
        ("GUI:Team", rect(300, 11, 240, 17)),
    ];
    for (row, columns) in layout.combos.iter_mut().enumerate() {
        let y = 36 + row as i32 * 26;
        columns[0] = rect(74, y, 180, 23);
        columns[4] = rect(300, y, 240, 23);
    }
    let bottom = 36 + row_count.min(layout.combos.len()) as i32 * 26;
    layout.roster_navigation = [rect(74, bottom, 94, 18), rect(446, bottom, 94, 18)];
    layout.roster_page_label = rect(176, bottom, 262, 18);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::multiplayer_setup::{BACK, CHOOSE_MAP, START, SetupFlavor};

    #[test]
    fn coop_reuses_original_sidebar_anchors_at_multiple_sizes() {
        let dialog = SetupDialog {
            host: true,
            flavor: SetupFlavor::Lan,
            modern: true,
        };
        for (width, height) in [(640, 480), (800, 600), (1920, 1080)] {
            let mut coop = MultiplayerSetupLayout::new(dialog, width, height, true);
            let native = MultiplayerSetupLayout::new(
                SetupDialog {
                    modern: false,
                    ..dialog
                },
                width,
                height,
                true,
            );
            apply(&mut coop, dialog, 4, width, height);
            assert_eq!(coop.page.title, native.page.title);
            assert_eq!(coop.map_preview, native.map_preview);
            assert_eq!(coop.mode_label, native.mode_label);
            assert_eq!(coop.map_label, native.map_label);
            assert_eq!(coop.chat, native.chat);
            assert_eq!(coop.chat_log, native.chat_log);
            for id in [START, CHOOSE_MAP, BACK] {
                assert_eq!(coop.page.button_rect(id), native.page.button_rect(id));
            }
            assert!(coop.combos[0][0].x + coop.combos[0][0].w < coop.combos[0][4].x);
        }
    }
}
