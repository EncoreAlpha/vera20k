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

/// One native list-tone rule, used for caption separators.
pub(crate) fn push_rule(
    out: &mut Vec<SpriteInstance>,
    atlas: &SkirmishShellChromeAtlas,
    rect: RectPx,
    scale: ShellScale,
    background_depth: f32,
) {
    crate::app::frontend::skirmish_shell_render::chrome::push_solid_rect(
        out,
        atlas,
        scale.scale_rect(rect),
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

/// Cut `text` to `max_w` native pixels and mark the cut with two dots, so a
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
        candidate.push_str("..");
        if font.text_width(&candidate) > max_w {
            break;
        }
        cut.push(ch);
    }
    cut.push_str("..");
    cut
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
    vec![first.trim_end().to_owned(), rest.trim_start().to_owned()]
}
