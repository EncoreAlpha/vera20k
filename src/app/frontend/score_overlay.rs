//! The shared score-page extension seam.
//!
//! The shared `0x103` / `0xBC6` renderers and their shared input feeds must
//! stay free of VMCE types and controls. They consume this module's generic
//! label/button/overlay data instead; the VMCE painter behind the seam turns
//! the open page's VMCE content into that data. Paint and hit-testing call the
//! same button projection, so the drawn and clickable button rectangles can
//! never drift apart.
//!
//! A page with no extension installed (ordinary Cooperative, ordinary
//! campaign victory) gets `None` here and keeps its native path untouched.

use crate::app::AppState;
use crate::app::cooperative_round::CoopScoreView;
use crate::render::batch::SpriteInstance;
use crate::render::shell_paint::PaintLabel;
use crate::render::shell_text::ShellAlign;
use crate::ui::campaign_score_shell::CampaignScoreLayout;
use crate::ui::coop_score_shell::CoopScoreLayout;
use crate::ui::shell::geom::RectPx;

/// One right-panel button on an extended `0x103` page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OverlayButton {
    /// The controller id the input feed presses.
    pub id: u16,
    pub rect: RectPx,
    /// The native pressed/lit frame (SDBTNANM frame 4).
    pub pressed: bool,
    pub enabled: bool,
    pub caption_key: &'static str,
    pub caption_rect: RectPx,
    pub caption_rgb: [f32; 3],
}

/// Generic overlay over the stock `0x103` page: extra labels, the extended
/// right-panel button projection, and board sprites drawn after the dialog
/// art.
#[derive(Default)]
pub(crate) struct CampaignScoreOverlay {
    /// Native static ids whose text is hidden; their reveals still advance.
    pub hidden_statics: Vec<u16>,
    /// The extension takes over the page body: no native static/cell labels.
    pub suppress_stock_text: bool,
    /// The page draws no native SCRT/SCRA art (statistics subpage).
    pub plain_backdrop: bool,
    pub labels: Vec<PaintLabel<'static>>,
    pub buttons: Vec<OverlayButton>,
    /// Board instances drawn with the skirmish shell chrome texture.
    pub board_sprites: Vec<SpriteInstance>,
}

/// One text line on an extended `0xBC6` page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CoopOverlayText {
    pub text: String,
    pub rect: RectPx,
    pub align: ShellAlign,
    pub rgb: [f32; 3],
}

/// One right-panel button on an extended `0xBC6` page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CoopOverlayButton {
    pub id: u16,
    /// The CSF/i18n key for the caption.
    pub label_key: &'static str,
    /// The native lit frame (frame 4) stays while the button is disabled.
    pub lit: bool,
    pub enabled: bool,
}

/// Generic overlay over the stock `0xBC6` page.
#[derive(Default)]
pub(crate) struct CoopScoreOverlay {
    /// The extension takes over the body: no native statics/table/cells.
    pub suppress_stock_text: bool,
    /// No chat, player-setup combos, slider or preview.
    pub suppress_body_controls: bool,
    /// Native control ids the extension hides.
    pub hidden_controls: Vec<u16>,
    pub texts: Vec<CoopOverlayText>,
    pub buttons: Vec<CoopOverlayButton>,
}

/// The `0x103` page's extension overlay, or `None` for a native page.
pub(crate) fn campaign_overlay(
    state: &AppState,
    layout: &CampaignScoreLayout,
) -> Option<CampaignScoreOverlay> {
    #[cfg(feature = "vmce")]
    {
        return crate::app::vmce_result::paint_victory::overlay(state, layout);
    }
    #[cfg(not(feature = "vmce"))]
    {
        let _ = (state, layout);
        None
    }
}

/// The single right-panel button projection of the open `0x103` page. Paint
/// and hit-testing both call this.
pub(crate) fn campaign_buttons(
    state: &AppState,
    layout: &CampaignScoreLayout,
) -> Option<Vec<OverlayButton>> {
    #[cfg(feature = "vmce")]
    {
        return crate::app::vmce_result::paint_victory::buttons(state, layout);
    }
    #[cfg(not(feature = "vmce"))]
    {
        let _ = (state, layout);
        None
    }
}

/// Route a released `0x103` control to the extension owner. False when the
/// page carries no extension or the id is not its control.
pub(crate) fn campaign_button_pressed(state: &mut AppState, released: Option<u16>) -> bool {
    #[cfg(feature = "vmce")]
    {
        return crate::app::vmce_result::button_pressed(state, released);
    }
    #[cfg(not(feature = "vmce"))]
    {
        let _ = (state, released);
        false
    }
}

/// The `0xBC6` page's extension overlay, or `None` for a native page.
pub(crate) fn coop_overlay(
    state: &AppState,
    layout: &CoopScoreLayout,
    view: &CoopScoreView,
) -> Option<CoopScoreOverlay> {
    #[cfg(feature = "vmce")]
    {
        return crate::app::vmce_result::paint_defeat::overlay(state, layout, view);
    }
    #[cfg(not(feature = "vmce"))]
    {
        let _ = (state, layout, view);
        None
    }
}

/// The single right-panel button projection of the open `0xBC6` page.
pub(crate) fn coop_buttons(
    state: &AppState,
    view: &CoopScoreView,
) -> Option<Vec<CoopOverlayButton>> {
    #[cfg(feature = "vmce")]
    {
        return crate::app::vmce_result::paint_defeat::buttons(state, view);
    }
    #[cfg(not(feature = "vmce"))]
    {
        let _ = (state, view);
        None
    }
}

/// Whether control `id` takes input on the open `0xBC6` page. `None` means the
/// page carries no extension and the native enablement applies.
pub(crate) fn coop_control_enabled(
    state: &AppState,
    view: &CoopScoreView,
    id: u16,
) -> Option<bool> {
    let buttons = coop_buttons(state, view)?;
    Some(
        buttons
            .iter()
            .find(|button| button.id == id)
            .is_some_and(|button| button.enabled),
    )
}

/// Whether the open `0xBC6` page is the statistics-only extension (no chat).
pub(crate) fn coop_is_statistics_only(state: &AppState) -> bool {
    #[cfg(feature = "vmce")]
    {
        return crate::app::vmce_result::defeat_page_active(state);
    }
    #[cfg(not(feature = "vmce"))]
    {
        let _ = state;
        false
    }
}

/// The row count and extra cell ids the open `0xBC6` page lays out. `None`
/// means the native two-row page.
pub(crate) fn coop_row_extension(state: &AppState) -> Option<(usize, &'static [[u16; 5]; 2])> {
    #[cfg(feature = "vmce")]
    {
        return crate::app::vmce_result::row_extension(state);
    }
    #[cfg(not(feature = "vmce"))]
    {
        let _ = state;
        None
    }
}
