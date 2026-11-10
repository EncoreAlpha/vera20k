//! Shared VERA VMCE panel chrome: one flat fill sampled from the native
//! dialog art, the native owner-draw list frame (`0x00619230`) and the small
//! text/icon helpers the lobby board and the settlement page both use. No new
//! bitmap art and no native-art retint.

use crate::render::batch::SpriteInstance;
use crate::render::bit_font::BitFont;
use crate::render::skirmish_shell_chrome::{SkirmishShellChromeAtlas, SkirmishShellChromeEntry};
use crate::ui::shell::geom::{RectPx, ShellScale};

/// Flat fill under VERA's panels, sampled from the native 451x326 dialog
/// art's flat dark interior (RGB565 2,4,3): one quiet dark tone behind the
/// native yellow text, with no tile seams.
pub(crate) const FILL_RGB: [f32; 3] = [16.0 / 255.0, 16.0 / 255.0, 25.0 / 255.0];

/// The empty campaign-role swatch: a muted neutral grey that stays readable on
/// [`FILL_RGB`] instead of reading as a hole.
pub(crate) const EMPTY_SWATCH_RGB: [f32; 3] = [0.28, 0.28, 0.30];

/// A swatch/frame hairline sampled from the native list frame's dark tone, so
/// VERA's fields use the same grey-blue bevel family as the native panels.
pub(crate) const SWATCH_OUTLINE_RGB: [f32; 3] = [0.41, 0.48, 0.50];

/// An inline status/error line: the native yellow is reserved for labels, so a
/// room that cannot start reads as a warning.
pub(crate) const WARNING_RGB: [f32; 3] = [1.0, 0.55, 0.22];

/// The depth of the flat fill: above the shell art, below the frame lines.
pub(crate) const FILL_DEPTH_OFFSET: f32 = 0.00006;

/// The depth of the native list frame lines.
pub(crate) const FRAME_DEPTH_OFFSET: f32 = 0.00007;

/// The depth of one list-tone rule.
pub(crate) const RULE_DEPTH_OFFSET: f32 = 0.00008;

/// The native art's own grown size, rounded and at least one pixel.
pub(crate) fn native_size(entry: SkirmishShellChromeEntry, scale: ShellScale) -> (i32, i32) {
    let [w, h] = scale.art_size(entry.pixel_size[0], entry.pixel_size[1]);
    (w.round().max(1.0) as i32, h.round().max(1.0) as i32)
}

pub(crate) fn push_solid_rect(
    out: &mut Vec<SpriteInstance>,
    atlas: &SkirmishShellChromeAtlas,
    rect: RectPx,
    rgb: [f32; 3],
    depth: f32,
) {
    crate::app::frontend::skirmish_shell_render::chrome::push_solid_rect(
        out, atlas, rect, rgb, depth,
    );
}

/// A translucent flat strip, used behind settlement row text.
pub(crate) fn push_solid_rect_alpha(
    out: &mut Vec<SpriteInstance>,
    atlas: &SkirmishShellChromeAtlas,
    rect: RectPx,
    rgb: [f32; 3],
    depth: f32,
    alpha: f32,
) {
    crate::app::frontend::skirmish_shell_render::chrome::push_solid_rect_alpha(
        out, atlas, rect, rgb, depth, alpha,
    );
}

/// One flat native dialog-palette fill inside the list frame.
pub(crate) fn push_fill(
    out: &mut Vec<SpriteInstance>,
    atlas: &SkirmishShellChromeAtlas,
    rect: RectPx,
    background_depth: f32,
) {
    crate::app::frontend::skirmish_shell_render::chrome::push_solid_rect(
        out,
        atlas,
        rect,
        FILL_RGB,
        background_depth - FILL_DEPTH_OFFSET,
    );
}

/// The bevel lines of one VMCE combo field, kept inside the cell so adjacent
/// rows and the owner board never lose their gutter to the frame's outer ring.
pub(crate) fn combo_field_lines(
    rect: RectPx,
    scale: ShellScale,
) -> [(RectPx, crate::ui::shell::list::ListFrameTone); 12] {
    let base = scale.base_rect(rect);
    // `list_frame_lines` grows its outer ring one pixel past the given window, so
    // the window is deflated by three to keep every ring line inside `rect`.
    let inner = RectPx::new(
        base.x + 1,
        base.y + 1,
        (base.w - 3).max(1),
        (base.h - 3).max(1),
    );
    crate::ui::shell::list::list_frame_lines(inner)
        .map(|(line, tone)| (scale.scale_rect(line), tone))
}

/// One editable field on VERA's native-toned panel chrome: the shared dark fill
/// inside the native list frame's two-tone bevel. The stock combo faces only
/// cover exact native widths (96 and 120 px have none), so VERA paints the field
/// itself instead of leaving a flat block. The bevel is inset so the field keeps
/// its own rect's gutters.
pub(crate) fn push_combo_field(
    out: &mut Vec<SpriteInstance>,
    atlas: &SkirmishShellChromeAtlas,
    rect: RectPx,
    scale: ShellScale,
    background_depth: f32,
) {
    push_fill(out, atlas, rect, background_depth);
    for (line, tone) in combo_field_lines(rect, scale) {
        crate::app::frontend::skirmish_shell_render::chrome::push_solid_rect(
            out,
            atlas,
            line,
            tone.rgb().map(|channel| f32::from(channel) / 255.0),
            background_depth - FRAME_DEPTH_OFFSET,
        );
    }
}

/// The native owner-draw list frame (`0x00619230`) around `rect`.
pub(crate) fn push_list_frame(
    out: &mut Vec<SpriteInstance>,
    atlas: &SkirmishShellChromeAtlas,
    rect: RectPx,
    scale: ShellScale,
    background_depth: f32,
) {
    for (line, tone) in crate::ui::shell::list::list_frame_lines(scale.base_rect(rect)) {
        crate::app::frontend::skirmish_shell_render::chrome::push_solid_rect(
            out,
            atlas,
            scale.scale_rect(line),
            tone.rgb().map(|channel| f32::from(channel) / 255.0),
            background_depth - FRAME_DEPTH_OFFSET,
        );
    }
}

/// One native list-tone rule, used for caption separators. `rect` is already in
/// device pixels, like [`push_fill`] and [`push_list_frame`].
pub(crate) fn push_rule(
    out: &mut Vec<SpriteInstance>,
    atlas: &SkirmishShellChromeAtlas,
    rect: RectPx,
    background_depth: f32,
) {
    crate::app::frontend::skirmish_shell_render::chrome::push_solid_rect(
        out,
        atlas,
        rect,
        crate::ui::shell::list::ListFrameTone::Dark
            .rgb()
            .map(|channel| f32::from(channel) / 255.0),
        background_depth - RULE_DEPTH_OFFSET,
    );
}

/// Native art at its own grown size, centred in `cell`: no crop, no stretch.
pub(crate) fn push_native_centred(
    out: &mut Vec<SpriteInstance>,
    entry: SkirmishShellChromeEntry,
    cell: RectPx,
    scale: ShellScale,
) {
    let (w, h) = native_size(entry, scale);
    crate::app::frontend::skirmish_shell_render::chrome::push_entry_sized(
        out,
        entry,
        (cell.x + (cell.w - w) / 2) as f32,
        (cell.y + (cell.h - h) / 2) as f32,
        [w as f32, h as f32],
        crate::app::frontend::skirmish_shell_render::SHELL_CONTROL_DEPTH,
    );
}

/// Cut `text` to `max_w` native pixels and mark the cut with three dots, so a
/// label never crosses its cell border.
pub(crate) fn ellipsize_native(font: &BitFont, text: &str, max_w: i32) -> String {
    let max_w = max_w.max(0) as u32;
    if font.text_width(text) <= max_w {
        return text.to_owned();
    }
    let mut cut = String::new();
    for ch in text.chars() {
        let mut candidate = cut.clone();
        candidate.push(ch);
        candidate.push_str("...");
        if font.text_width(&candidate) > max_w {
            break;
        }
        cut.push(ch);
    }
    cut.push_str("...");
    cut
}

/// [`ellipsize_native`] for a device-space cell: the font measures authored
/// pixels, so a scaled rect has to be divided back before it is compared.
pub(crate) fn ellipsize_scaled(
    font: &BitFont,
    text: &str,
    rect: RectPx,
    scale: ShellScale,
) -> String {
    ellipsize_native(font, text, scale.base_rect(rect).w.max(1))
}

/// Cut `text` into at most two native-pixel lines, preferring an ASCII space,
/// so a strip reads as two leading-separated rows.
pub(crate) fn wrap_two_lines(font: &BitFont, text: &str, max_w: i32) -> Vec<String> {
    let max_w = max_w.max(1) as u32;
    if font.text_width(text) <= max_w {
        return vec![text.to_owned()];
    }
    let mut best = 0;
    for (index, _) in text.char_indices() {
        if font.text_width(&text[..index]) > max_w {
            break;
        }
        best = index;
    }
    let split = text[..best]
        .rfind(' ')
        .map(|space| space + 1)
        .filter(|&split| split > 0)
        .unwrap_or_else(|| {
            text.char_indices()
                .nth(1)
                .map(|(index, _)| index)
                .unwrap_or(text.len())
        });
    let (first, rest) = text.split_at(split);
    let first = first.trim_end().to_owned();
    let rest = rest.trim_start();
    let rest = if font.text_width(rest) > max_w {
        ellipsize_native(font, rest, max_w as i32)
    } else {
        rest.to_owned()
    };
    vec![first, rest]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::bit_font::tests::make_test_font;

    #[test]
    fn the_cut_is_marked_with_three_dots() {
        let font = make_test_font(&[(b'a' as u16, 6)], 4);
        let cut = ellipsize_native(&font, "aaaaaaaaaaaaaaaa", 40);
        assert!(cut.ends_with("..."), "{cut}");
        assert!(font.text_width(&cut) <= 40, "cut runs past its cell: {cut}");
    }

    #[test]
    fn a_scaled_cell_is_measured_in_authored_pixels() {
        let font = make_test_font(&[(b'a' as u16, 6)], 4);
        let scale = ShellScale::from_screen(1600, 1200);
        let cell = scale.scale_rect(RectPx::new(0, 0, 40, 22));
        let cut = ellipsize_scaled(&font, "aaaaaaaaaaaaaaaa", cell, scale);
        assert!(font.text_width(&cut) <= 40, "cut runs past its cell: {cut}");
        assert!(font.text_width(&cut) < font.text_width("aaaaaaaaaaaaaaaa"));
    }

    #[test]
    fn a_long_second_line_ends_with_three_dots() {
        let font = make_test_font(&[(b'a' as u16, 6)], 4);
        let lines = wrap_two_lines(&font, "aaaa bbbbbbbbbbbbbbbb", 40);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with("..."), "{}", lines[1]);
        assert!(font.text_width(&lines[1]) <= 40);
    }

    #[test]
    fn the_combo_field_bevel_stays_inside_its_cell() {
        let scale = ShellScale::from_screen(1600, 1200);
        let cell = scale.scale_rect(RectPx::new(74, 36, 96, 24));
        let base = scale.base_rect(cell);
        for (line, _) in combo_field_lines(cell, scale) {
            let line = scale.base_rect(line);
            assert!(
                line.x >= base.x
                    && line.y >= base.y
                    && line.x + line.w <= base.x + base.w
                    && line.y + line.h <= base.y + base.h,
                "{line:?} outside {base:?}"
            );
        }
    }
}
