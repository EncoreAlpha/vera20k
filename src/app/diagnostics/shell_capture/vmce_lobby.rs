//! VMCE lobby shell-capture states.
//!
//! The room is installed from the diagnostics fixture in the production
//! session/UI owners (`install_vmce_lobby_fixture`) and painted by the ordinary
//! setup renderer; there is no live peer and no map file read. This is a
//! synthetic UI review, not native-parity evidence.

use super::*;
use crate::app::shell_network_setup::{VmceLobbyFixture, VmceLobbyStatus};
use serde_json::{Value, json};

/// Frames to wait after a state install before the readback, so the entry
/// slide clears and every reveal settles.
const SETTLE_FRAMES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmceLobbyTarget {
    /// The host with four admitted humans, every authored role claimed.
    HostFourHumans,
    /// A guest that has not readied yet.
    GuestWaiting,
    /// A guest whose local map/campaign install is missing.
    InlineError,
    /// The host's blocking invalid-campaign message box.
    BlockingError,
}

impl VmceLobbyTarget {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 4] = [
        Self::HostFourHumans,
        Self::GuestWaiting,
        Self::InlineError,
        Self::BlockingError,
    ];

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "vmce-lobby-host" => Some(Self::HostFourHumans),
            "vmce-lobby-guest-waiting" => Some(Self::GuestWaiting),
            "vmce-lobby-inline-error" => Some(Self::InlineError),
            "vmce-lobby-blocking-error" => Some(Self::BlockingError),
            _ => None,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::HostFourHumans => "vmce-lobby-host",
            Self::GuestWaiting => "vmce-lobby-guest-waiting",
            Self::InlineError => "vmce-lobby-inline-error",
            Self::BlockingError => "vmce-lobby-blocking-error",
        }
    }

    fn fixture(self) -> VmceLobbyFixture {
        match self {
            Self::HostFourHumans => VmceLobbyFixture {
                host: true,
                claimed_roles: 4,
                peer_ready: true,
                status: None,
                blocking: None,
            },
            Self::GuestWaiting => VmceLobbyFixture {
                host: false,
                claimed_roles: 1,
                peer_ready: true,
                status: None,
                blocking: None,
            },
            Self::InlineError => VmceLobbyFixture {
                host: false,
                claimed_roles: 1,
                peer_ready: false,
                status: Some(VmceLobbyStatus::MissingAssets),
                blocking: None,
            },
            Self::BlockingError => VmceLobbyFixture {
                host: true,
                claimed_roles: 4,
                peer_ready: true,
                status: None,
                blocking: Some(crate::i18n::tr_fill(
                    "VERA:VmceBlockingInvalidCampaign",
                    &[("map", &"VMCE/Capture/mission.map")],
                )),
            },
        }
    }
}

pub(crate) struct VmceLobbyCapture {
    target: VmceLobbyTarget,
    route: Vec<Value>,
    settle: Option<u32>,
}

impl VmceLobbyCapture {
    pub(crate) fn new(target: VmceLobbyTarget) -> Self {
        Self {
            target,
            route: Vec::new(),
            settle: None,
        }
    }

    pub(crate) fn prepare(&mut self, state: &mut AppState) -> Result<()> {
        ensure!(
            crate::app::App::ensure_skirmish_shell_chrome(state),
            "VMCE lobby capture needs the retail shell art"
        );
        crate::app::shell_network_setup::install_vmce_lobby_fixture(state, &self.target.fixture())?;
        let fixture = self.target.fixture();
        self.route.push(json!({
            "state": self.target.as_str(),
            "fixture_kind": "synthetic UI review; no network match or map file",
            "host": fixture.host,
            "claimed_roles": fixture.claimed_roles,
            "peer_ready": fixture.peer_ready,
            "inline_status": fixture.status.map(|status| match status {
                VmceLobbyStatus::MissingAssets => "missing-map-or-campaign",
                VmceLobbyStatus::MapMismatch => "map-or-series-mismatch",
            }),
            "blocking": fixture.blocking,
        }));
        Ok(())
    }

    pub(crate) fn after_present(&mut self, state: &AppState) -> Result<()> {
        if self.settle.is_none() && self.ready_now(state)? {
            self.settle = Some(SETTLE_FRAMES);
        } else if let Some(remaining) = self.settle {
            self.settle = Some(remaining.saturating_sub(1));
        }
        Ok(())
    }

    pub(crate) fn ready(&self, state: &AppState) -> Result<bool> {
        Ok(self.settle == Some(0)
            && state.frontend.shell_first_paint_slide.is_none()
            && state.frontend.shell_exit.is_none()
            && self.ready_now(state)?)
    }

    fn ready_now(&self, state: &AppState) -> Result<bool> {
        ensure!(
            state.frontend.shell_route.network_setup(),
            "VMCE lobby capture left the setup route"
        );
        let Some(view) = crate::app::shell_network_setup::view(state) else {
            return Ok(false);
        };
        ensure!(view.vmce, "VMCE lobby capture lost its mode");
        match self.target {
            VmceLobbyTarget::HostFourHumans => {
                ensure!(view.host, "host capture admitted a guest");
                ensure!(
                    view.vmce_roles.iter().all(|slot| matches!(
                        slot,
                        crate::ui::multiplayer_setup::VmceLobbyRoleSlot::Claimed(_)
                    )),
                    "host capture has an unclaimed authored role"
                );
                ensure!(view.vmce_status.is_none(), "host capture has a warning");
            }
            VmceLobbyTarget::GuestWaiting => {
                ensure!(!view.host, "guest capture admitted a host");
                ensure!(
                    view.vmce_roles.iter().any(|slot| matches!(
                        slot,
                        crate::ui::multiplayer_setup::VmceLobbyRoleSlot::Waiting
                    )),
                    "waiting guest must show an unclaimed row"
                );
            }
            VmceLobbyTarget::InlineError => {
                ensure!(view.vmce_status.is_some(), "inline error has no status");
            }
            VmceLobbyTarget::BlockingError => {
                ensure!(
                    state
                        .frontend
                        .multiplayer_setup
                        .as_ref()
                        .is_some_and(|ui| ui.notice.is_some()),
                    "blocking capture has no message box"
                );
            }
        }
        Ok(true)
    }

    pub(crate) fn manifest(
        &self,
        request: &ShellCaptureRequest,
        format: wgpu::TextureFormat,
        pixels: &[u8],
        frame: u32,
        state: &AppState,
    ) -> Value {
        let fixture = self.target.fixture();
        let view = crate::app::shell_network_setup::view(state);
        let board = view.as_ref().map(|view| {
            view.vmce_roles
                .iter()
                .enumerate()
                .map(|(index, slot)| match slot {
                    crate::ui::multiplayer_setup::VmceLobbyRoleSlot::Waiting => json!({
                        "row": index + 1,
                        "state": "waiting",
                    }),
                    crate::ui::multiplayer_setup::VmceLobbyRoleSlot::Claimed(role) => json!({
                        "row": index + 1,
                        "role": role.role,
                        "country": role.country,
                        "flag": role.flag,
                        "color": role.color,
                        "host": role.host,
                        "ready": role.ready,
                    }),
                })
                .collect::<Vec<_>>()
        });
        json!({
            "schema_version": "vera20k.vmce-lobby-shell-capture.v1",
            "checkpoint": request.checkpoint.as_str(),
            "target": self.target.as_str(),
            "fixture_kind": "synthetic UI review; no network match or map file",
            "fixture": {
                "host": fixture.host,
                "claimed_roles": fixture.claimed_roles,
                "peer_ready": fixture.peer_ready,
            },
            "state": {
                "mode": "vmce",
                "host": view.as_ref().map(|view| view.host),
                "rows": board,
                "mode_label": view.as_ref().map(|view| &view.mode_label),
                "map_label": view.as_ref().map(|view| &view.map_label),
                "inline_status": view.as_ref().and_then(|view| view.vmce_status.as_deref()),
                "blocking_message": fixture.blocking,
            },
            "parity_certification": "NONE",
            "presenter_domain": "final-swapchain-after-rgb565",
            "surface": {"width": request.width, "height": request.height,
                "format": format!("{format:?}"), "pixel_layout": "BGRA8",
                "row_order": "top-left", "row_stride": request.width * 4},
            "cursor": {"x": request.cursor_x, "y": request.cursor_y,
                "policy": "software-composited"},
            "route": self.route,
            "capture_frame": frame,
            "frame": {"path": FRAME_FILE_NAME, "byte_length": pixels.len(),
                "sha256": crate::util::sha256::sha256_hex(pixels)},
            "png": {"path": "frame.png"},
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_vmce_lobby_checkpoint_round_trips_its_name() {
        for target in VmceLobbyTarget::ALL {
            assert_eq!(VmceLobbyTarget::from_name(target.as_str()), Some(target));
        }
        assert_eq!(VmceLobbyTarget::from_name("vmce-lobby-unknown"), None);
    }

    /// The scenario's own writer produces a non-empty image and receipt; the
    /// frame here is a tiny fixture, so this proves the capture route's
    /// plumbing, not any pixel or layout.
    #[test]
    fn the_capture_plumbing_writes_a_non_empty_image_and_receipt() {
        let dir = std::env::temp_dir().join(format!(
            "vera20k-vmce-lobby-capture-smoke-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        write_vmce_lobby_frame_png(
            &dir,
            2,
            2,
            &[
                0u8, 0, 255, 255, 0, 255, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255,
            ],
        )
        .expect("write image");
        let receipt = json!({
            "schema_version": "vera20k.vmce-lobby-shell-capture.v1",
            "target": VmceLobbyTarget::HostFourHumans.as_str(),
            "state": {"mode": "vmce", "rows": []},
        });
        write_new_file(
            &dir.join(MANIFEST_FILE_NAME),
            &serde_json::to_vec_pretty(&receipt).expect("serialize"),
        )
        .expect("write receipt");
        let png = std::fs::read(dir.join("frame.png")).expect("read png");
        let receipt = std::fs::read(dir.join(MANIFEST_FILE_NAME)).expect("read receipt");
        assert!(!png.is_empty());
        assert!(!receipt.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Encode one captured BGRA8 frame as the PNG written beside the bundle.
pub(crate) fn write_vmce_lobby_frame_png(
    dir: &std::path::Path,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<()> {
    let expected = usize::try_from(u64::from(width) * u64::from(height) * 4)?;
    ensure!(
        pixels.len() == expected,
        "VMCE lobby frame is not a complete BGRA8 image"
    );
    let mut rgba = Vec::with_capacity(pixels.len());
    for bgra in pixels.chunks_exact(4) {
        rgba.extend_from_slice(&[bgra[2], bgra[1], bgra[0], bgra[3]]);
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(
        &rgba,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )?;
    std::fs::create_dir_all(dir)?;
    write_new_file(&dir.join("frame.png"), &png)
}
