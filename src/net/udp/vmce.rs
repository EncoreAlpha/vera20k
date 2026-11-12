//! VMCE's intermission transport over the direct-LAN UDP owner.
//!
//! While peers cross the result-dialog boundary, the previous match's reliable
//! global channel keeps retrying and ACKing its control while a fresh lobby
//! channel carries the next setup. This module is compiled only with the
//! capability: the shared UDP owner keeps a small set of gated access points
//! and no intermission state when the feature is off.

use super::*;

/// The previous match's global channel, buffered beside the lobby's one until
/// the application retires the match.
#[derive(Debug)]
pub(super) struct LanIntermissionTransport {
    pub(super) scope: SessionScope,
    pub(super) other: GlobalChannel,
    pub(super) entered: bool,
}

/// A new LAN match scope retires an entered intermission unless it reuses the
/// previous epoch; the app's accepted GO proves the host crossed the boundary.
pub(super) fn prepare_scope(io: &mut UdpIo, scope: SessionScope) -> io::Result<()> {
    if let Some(transition) = io
        .lan_intermission
        .as_ref()
        .filter(|transition| transition.entered)
    {
        if transition.scope.epoch == scope.epoch {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "new LAN match reused the previous epoch",
            ));
        }
        // The app has accepted this new room's GO and its epoch. That proves
        // the host crossed the previous GO/ACK boundary even if no setup
        // heartbeat was observed before the room started.
        io.lan_intermission = None;
    }
    Ok(())
}

pub(super) fn transitioning(io: &UdpIo) -> bool {
    io.lan_intermission.is_some()
}

pub(super) fn clear(io: &mut UdpIo) {
    io.lan_intermission = None;
}

/// The scope a scoped datagram is validated against: the live match first,
/// then the retired match whose control the intermission still ACKs.
pub(super) fn scope_or_intermission(
    scope: Option<SessionScope>,
    intermission: &Option<LanIntermissionTransport>,
) -> Option<SessionScope> {
    scope.or_else(|| {
        intermission
            .as_ref()
            .filter(|transition| transition.entered)
            .map(|transition| transition.scope)
    })
}

/// Whether a received global frame belongs to the retired match's channel.
pub(super) fn other_channel(
    intermission: &Option<LanIntermissionTransport>,
    received_scope: Option<SessionScope>,
) -> bool {
    intermission.as_ref().is_some_and(|transition| {
        if transition.entered {
            received_scope == Some(transition.scope)
        } else {
            received_scope.is_none()
        }
    })
}

/// The global channel a received frame is delivered to.
pub(super) fn receiving<'a>(
    global: &'a mut GlobalChannel,
    intermission: &'a mut Option<LanIntermissionTransport>,
    other: bool,
) -> &'a mut GlobalChannel {
    if other {
        return &mut intermission.as_mut().expect("other channel").other;
    }
    &mut *global
}

/// The retired match's channel only ACKs/retries its reliable control; no old
/// payload reaches the new room.
pub(super) fn drain_retired_control(
    intermission: &mut Option<LanIntermissionTransport>,
    other: bool,
) {
    if other
        && intermission
            .as_ref()
            .is_some_and(|transition| transition.entered)
    {
        while intermission
            .as_mut()
            .expect("other channel")
            .other
            .pop_received()
            .is_some()
        {}
    }
}

/// Service the retired match's global channel once per pass, as the native
/// manager services the current one.
pub(super) fn service_transition(io: &mut UdpIo, now_ticks: u32, report: &mut UdpServiceReport) {
    if let Some(transition) = io.lan_intermission.as_mut() {
        let scope = transition.entered.then_some(transition.scope);
        if !transition
            .other
            .service(now_ticks, &mut |destination, frame| {
                send_global_frame(
                    &io.primary,
                    &mut io.relay,
                    scope,
                    UdpChannel::Primary,
                    &mut io.primary_send,
                    destination,
                    &io.broadcast_targets,
                    frame,
                    report,
                );
            })
        {
            report.discarded_global_sends += transition.other.discard_failed_sends();
        }
    }
}

impl UdpIo {
    /// Prepare a separate plain lobby channel while the match scope remains
    /// active. Early lobby packets are buffered until `enter_lan_intermission`.
    fn prepare_lan_intermission(&mut self) -> io::Result<()> {
        if self.lan_intermission.is_some() {
            return Ok(());
        }
        let Some(LanScopeState::Active(scope)) = self.lan_scope else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no active LAN match scope",
            ));
        };
        let other = GlobalChannel::new(
            GlobalConfig {
                product_id: self.connections.product_id(),
                ..GlobalConfig::default()
            },
            self.connections.retry_timing(),
        );
        self.lan_intermission = Some(LanIntermissionTransport {
            scope,
            other,
            entered: false,
        });
        Ok(())
    }

    /// Switch application traffic to the buffered lobby channel without
    /// dropping the previous scope's retry and duplicate-ACK state.
    fn enter_lan_intermission(&mut self) -> io::Result<()> {
        let Some(transition) = self.lan_intermission.as_mut() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "LAN intermission was not prepared",
            ));
        };
        if !transition.entered {
            std::mem::swap(&mut self.global, &mut transition.other);
            transition.entered = true;
            self.connections.reset();
            self.lan_scope = None;
        }
        Ok(())
    }

    /// Retire the previous match only after the application has evidence that
    /// the peer received its last scoped control and entered the new lobby.
    fn finish_lan_intermission(&mut self) {
        if self
            .lan_intermission
            .as_ref()
            .is_some_and(|transition| transition.entered)
        {
            self.lan_intermission = None;
        }
    }
}

impl LanUdpTransport {
    /// Prepare a separate plain lobby channel while the match scope remains
    /// active. Early lobby packets are buffered until `enter_lan_intermission`.
    pub fn prepare_lan_intermission(&mut self) -> io::Result<()> {
        self.lock().prepare_lan_intermission()
    }

    /// Switch application traffic to the buffered lobby channel without
    /// dropping the previous scope's retry and duplicate-ACK state.
    pub fn enter_lan_intermission(&mut self) -> io::Result<()> {
        self.lock().enter_lan_intermission()
    }

    /// Retire the previous match only after the application has evidence that
    /// the peer received its last scoped control and entered the new lobby.
    pub fn finish_lan_intermission(&mut self) {
        self.lock().finish_lan_intermission();
    }

    /// A guest that received Go no longer needs to retry prior result-dialog
    /// sends, but must keep the old receive dedup state to ACK repeated Go.
    pub fn cancel_retired_lan_sends_to(&mut self, endpoint: SocketAddrV4) {
        if let Some(transition) = self.lock().lan_intermission.as_mut()
            && transition.entered
        {
            transition.other.cancel_sends_to_endpoint(endpoint);
        }
    }
}
