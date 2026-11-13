//! VMCE's own `0xBC6` defeat/retry dialog geometry.
//!
//! The native dialog and its right-panel slots are reproduced here from the
//! executed `0xBC6` layout; no Cooperative page model participates. The page
//! content and actions belong to [`crate::app::vmce_result::defeat`].

use crate::ui::shell::descriptor::{DialogId, LOW_HEADING_ANCHOR};
use crate::ui::shell::geom::{RectPx, ShellScale};
use crate::ui::shell::layout::{anchor_rect, status_line_rect};
use crate::ui::shell::menu_page::{self, MenuPageButtonSpec, MenuPageLayout, MenuPageSpec};

/// The native defeat/retry dialog id.
pub(crate) const VMCE_DEFEAT_DIALOG: DialogId = DialogId(0x0BC6);
/// The right-panel Retry slot (the native Continue control id).
pub(crate) const RETRY: u16 = 0x06D1;
/// The right-panel Leave Room slot (the native Quit control id).
pub(crate) const LEAVE: u16 = 0x05C0;

const PAGE: MenuPageSpec = MenuPageSpec {
    dialog: VMCE_DEFEAT_DIALOG,
    title_key: "GUI:CampaignResults",
    stacked: &[MenuPageButtonSpec {
        id: RETRY,
        dlu_top: 149,
        csf_key: "GUI:Continue",
        tooltip_key: "STT:MPCampScoreContinue",
        result: None,
    }],
    back: MenuPageButtonSpec {
        id: LEAVE,
        dlu_top: 346,
        csf_key: "GUI:Quit",
        tooltip_key: "STT:MPCampScoreQuit",
        result: None,
    },
};

/// The VMCE defeat page's dialog layout at the current screen scale.
#[derive(Debug, Clone)]
pub(crate) struct VmceDefeatLayout {
    pub(crate) page: MenuPageLayout,
}

impl VmceDefeatLayout {
    pub(crate) fn new(width: i32, height: i32, responsive: bool) -> Self {
        let mut page = if responsive {
            menu_page::compute_responsive_layout(&PAGE, width as u32, height as u32)
        } else {
            menu_page::compute_layout(&PAGE, width as u32, height as u32)
        };
        let scale = page.scale;
        let (aw, ah) = if responsive {
            (800, 600)
        } else {
            (width, height)
        };
        page.title = scale.scale_rect(anchor_rect(
            LOW_HEADING_ANCHOR,
            RectPx::new(425, 1, 107, 10),
            aw,
            ah,
        ));
        page.status_help = scale.scale_rect(status_line_rect(RectPx::new(1, 356, 316, 12), aw, ah));
        Self { page }
    }

    pub(crate) fn scale(&self) -> ShellScale {
        self.page.scale
    }

    pub(crate) fn button_rect(&self, id: u16) -> Option<RectPx> {
        self.page
            .buttons
            .iter()
            .find(|button| button.id == id)
            .map(|button| button.rect)
    }
}
