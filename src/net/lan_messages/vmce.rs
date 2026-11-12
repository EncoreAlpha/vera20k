//! VMCE-only records on the global connection envelope.
//!
//! Kinds `0x36..0x3E` are VERA-only, outside the native BC6 records: the
//! intermission identity, the terminal-result agreement, round control, the
//! VMCE options record and the next-stage identity. The wire layout is
//! unchanged from the pre-split module; the shared `cooperative` module keeps
//! only the native BC6 records.

use super::cooperative::{CooperativeOptionsRecord, OPTIONS_KIND, SIGNAL_LEN};
use super::{LAN_MESSAGE_LEN, LanMessage, LanMessageError, LanMessageKind};

/// VMCE-only new lobby identity, delivered reliably over the previous match's
/// scope before Go. The revision starts at zero for the normal setup owner.
pub const INTERMISSION_KIND: u32 = 0x3B;

/// A VMCE terminal-result agreement, scoped to this result round. Each byte
/// is either an empty raw slot (0xff), or role 0..3 plus bit 2 for victory.
pub const RESULT_KIND: u32 = 0x3C;

/// VMCE control messages carry the result agreement's round nonce. These
/// kinds are distinct from the native BC6 records, which remain unchanged.
pub const VMCE_CONTROL_KIND: u32 = 0x3D;
pub const VMCE_OPTIONS_KIND: u32 = 0x3E;
const VMCE_OPTIONS_NONCE: usize = 0x130;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmceRoundControl {
    Begin,
    Ack,
    Go,
}

impl VmceRoundControl {
    pub fn encode(self, name: &str, nonce: [u8; 16]) -> Result<LanMessage, LanMessageError> {
        if nonce == [0; 16] {
            return Err(LanMessageError::InvalidStageRecord);
        }
        let mut message = LanMessage::with_name(LanMessageKind::Unknown(VMCE_CONTROL_KIND), name)?;
        message.bytes[SIGNAL_LEN] = match self {
            Self::Begin => 0,
            Self::Ack => 1,
            Self::Go => 2,
        };
        message.bytes[SIGNAL_LEN + 1..SIGNAL_LEN + 17].copy_from_slice(&nonce);
        Ok(message)
    }

    pub fn decode(message: &LanMessage) -> Option<(Self, [u8; 16])> {
        if message.u32(0) != VMCE_CONTROL_KIND {
            return None;
        }
        let control = match message.bytes[SIGNAL_LEN] {
            0 => Self::Begin,
            1 => Self::Ack,
            2 => Self::Go,
            _ => return None,
        };
        let nonce: [u8; 16] = message.bytes[SIGNAL_LEN + 1..SIGNAL_LEN + 17]
            .try_into()
            .ok()?;
        (nonce != [0; 16]).then_some((control, nonce))
    }
}

/// VMCE-only header signals: a guest's next stage differs from the host's
/// (`0x37`), this peer resolved no next stage (`0x38`), or a guest votes to
/// retry the BC6 defeat page (`0x39`). Stock BC6 has none of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmceSignal {
    StageRejected,
    StageAbsent,
    RetryVote,
}

impl VmceSignal {
    pub const fn kind(self) -> u32 {
        match self {
            Self::StageRejected => 0x37,
            Self::StageAbsent => 0x38,
            Self::RetryVote => 0x39,
        }
    }

    /// The kind dword, then the narrow name from `+0x04`, as the native
    /// header-only BC6 signals are laid out.
    pub fn encode(self, name: &str) -> Result<[u8; SIGNAL_LEN], LanMessageError> {
        let message = LanMessage::with_name(LanMessageKind::Unknown(self.kind()), name)?;
        Ok(message.as_bytes()[..SIGNAL_LEN]
            .try_into()
            .expect("signal prefix"))
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != SIGNAL_LEN {
            return None;
        }
        match u32::from_le_bytes(bytes[..4].try_into().expect("kind")) {
            0x37 => Some(Self::StageRejected),
            0x38 => Some(Self::StageAbsent),
            0x39 => Some(Self::RetryVote),
            _ => None,
        }
    }
}

/// The result record's fields end before the LAN message's payload does, so
/// the digest never truncates on the wire.
const _: () = assert!(SIGNAL_LEN + 21 + 32 <= LAN_MESSAGE_LEN);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmceResultRecord {
    pub nonce: [u8; 16],
    pub entries: [u8; 4],
    pub confirmed: bool,
    /// Digest of this round's resolved continuation, bound to the result nonce.
    pub stage_digest: [u8; 32],
}

impl VmceResultRecord {
    pub fn valid(&self) -> bool {
        // A zero nonce or digest is an absent field: the pre-digest record
        // and a replayed buffer whose tail never got written are not results.
        if self.nonce == [0; 16] || self.stage_digest == [0; 32] {
            return false;
        }
        let mut roles = 0u8;
        let mut count = 0;
        for value in self.entries {
            if value == 0xff {
                continue;
            }
            if value > 7 || roles & (1 << (value & 3)) != 0 {
                return false;
            }
            roles |= 1 << (value & 3);
            count += 1;
        }
        count > 0
    }

    pub fn encode(&self, name: &str) -> Result<LanMessage, LanMessageError> {
        if !self.valid() {
            return Err(LanMessageError::InvalidStageRecord);
        }
        let mut message = LanMessage::with_name(LanMessageKind::Unknown(RESULT_KIND), name)?;
        message.bytes[SIGNAL_LEN..SIGNAL_LEN + 16].copy_from_slice(&self.nonce);
        message.bytes[SIGNAL_LEN + 16..SIGNAL_LEN + 20].copy_from_slice(&self.entries);
        message.bytes[SIGNAL_LEN + 20] = u8::from(self.confirmed);
        message.bytes[SIGNAL_LEN + 21..SIGNAL_LEN + 53].copy_from_slice(&self.stage_digest);
        Ok(message)
    }

    pub fn decode(message: &LanMessage) -> Option<Self> {
        if message.u32(0) != RESULT_KIND {
            return None;
        }
        let confirmed = match message.bytes[SIGNAL_LEN + 20] {
            0 => false,
            1 => true,
            _ => return None,
        };
        let record = Self {
            nonce: message.bytes[SIGNAL_LEN..SIGNAL_LEN + 16].try_into().ok()?,
            entries: message.bytes[SIGNAL_LEN + 16..SIGNAL_LEN + 20]
                .try_into()
                .ok()?,
            confirmed,
            stage_digest: message.bytes[SIGNAL_LEN + 21..SIGNAL_LEN + 53]
                .try_into()
                .ok()?,
        };
        record.valid().then_some(record)
    }
}

pub fn encode_intermission_epoch(
    name: &str,
    epoch: [u8; 16],
) -> Result<LanMessage, LanMessageError> {
    if epoch == [0; 16] {
        return Err(LanMessageError::InvalidStageRecord);
    }
    let mut message = LanMessage::with_name(LanMessageKind::Unknown(INTERMISSION_KIND), name)?;
    message.bytes[SIGNAL_LEN..SIGNAL_LEN + 16].copy_from_slice(&epoch);
    Ok(message)
}

pub fn decode_intermission_epoch(message: &LanMessage) -> Option<[u8; 16]> {
    if message.u32(0) != INTERMISSION_KIND {
        return None;
    }
    let epoch: [u8; 16] = message.bytes[SIGNAL_LEN..SIGNAL_LEN + 16].try_into().ok()?;
    (epoch != [0; 16]).then_some(epoch)
}

#[test]
fn intermission_identity_requires_a_nonzero_epoch() {
    let message = encode_intermission_epoch("Host", [7; 16]).unwrap();
    let received = LanMessage::decode(message.as_bytes()).unwrap();
    assert_eq!(decode_intermission_epoch(&received), Some([7; 16]));
    assert!(encode_intermission_epoch("Host", [0; 16]).is_err());
    assert!(
        decode_intermission_epoch(
            &LanMessage::with_name(LanMessageKind::Unknown(0x32), "Host").unwrap()
        )
        .is_none()
    );
}

/// VERA-only record (`0x36`, not a native BC6 kind): the host's resolved
/// next stage identity, sent before the native begin so a guest whose own
/// campaign resolved a different stage can refuse before either peer loads
/// it. The digests are the hex strings the local resolvers already produce.
pub const STAGE_KIND: u32 = 0x36;
const STAGE_OFFSET: usize = 0x2f;
const STAGE_MAP_CAPACITY: usize = 0xd7;
const STAGE_SOURCE_OFFSET: usize = STAGE_OFFSET + 1 + STAGE_MAP_CAPACITY;
const STAGE_EFFECTIVE_OFFSET: usize = STAGE_SOURCE_OFFSET + 64;
const STAGE_CONFIG_OFFSET: usize = STAGE_EFFECTIVE_OFFSET + 64;
const STAGE_END: usize = STAGE_CONFIG_OFFSET + 64;
const _: () = assert!(STAGE_END <= LAN_MESSAGE_LEN);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmceStageRecord {
    pub map_file: String,
    pub source_sha256: String,
    pub effective_digest: String,
    pub config_sha256: String,
}

impl VmceStageRecord {
    pub fn encode(&self, name: &str) -> Result<LanMessage, LanMessageError> {
        let map = self.map_file.as_bytes();
        if map.is_empty() || map.len() > STAGE_MAP_CAPACITY {
            return Err(LanMessageError::TextTooLong {
                field: "VMCE stage map",
                actual: map.len(),
                maximum: STAGE_MAP_CAPACITY,
            });
        }
        for digest in [
            &self.source_sha256,
            &self.effective_digest,
            &self.config_sha256,
        ] {
            if !digest_is_valid(digest) {
                return Err(LanMessageError::InvalidStageRecord);
            }
        }
        let mut message = LanMessage::with_name(LanMessageKind::Unknown(STAGE_KIND), name)?;
        message.bytes[STAGE_OFFSET] = map.len() as u8;
        message.bytes[STAGE_OFFSET + 1..STAGE_OFFSET + 1 + map.len()].copy_from_slice(map);
        for (offset, digest) in [
            (STAGE_SOURCE_OFFSET, &self.source_sha256),
            (STAGE_EFFECTIVE_OFFSET, &self.effective_digest),
            (STAGE_CONFIG_OFFSET, &self.config_sha256),
        ] {
            message.bytes[offset..offset + 64].copy_from_slice(digest.as_bytes());
        }
        Ok(message)
    }

    pub fn decode(message: &LanMessage) -> Option<Self> {
        if message.u32(0) != STAGE_KIND {
            return None;
        }
        let map_len = usize::from(message.bytes[STAGE_OFFSET]);
        if map_len == 0 || map_len > STAGE_MAP_CAPACITY {
            return None;
        }
        let map_file =
            std::str::from_utf8(&message.bytes[STAGE_OFFSET + 1..STAGE_OFFSET + 1 + map_len])
                .ok()?
                .to_owned();
        let digest = |offset: usize| {
            std::str::from_utf8(&message.bytes[offset..offset + 64])
                .ok()
                .filter(|text| digest_is_valid(text))
                .map(str::to_owned)
        };
        Some(Self {
            map_file,
            source_sha256: digest(STAGE_SOURCE_OFFSET)?,
            effective_digest: digest(STAGE_EFFECTIVE_OFFSET)?,
            config_sha256: digest(STAGE_CONFIG_OFFSET)?,
        })
    }
}

fn digest_is_valid(digest: &str) -> bool {
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl CooperativeOptionsRecord {
    pub fn encode_vmce(&self, name: &str, nonce: [u8; 16]) -> Result<LanMessage, LanMessageError> {
        if nonce == [0; 16] {
            return Err(LanMessageError::InvalidStageRecord);
        }
        let mut message = self.encode(name)?;
        message.put_u32(0, VMCE_OPTIONS_KIND);
        message.bytes[VMCE_OPTIONS_NONCE..VMCE_OPTIONS_NONCE + 16].copy_from_slice(&nonce);
        Ok(message)
    }

    pub fn decode_vmce(message: &LanMessage) -> Option<(Self, [u8; 16])> {
        if message.u32(0) != VMCE_OPTIONS_KIND {
            return None;
        }
        let nonce: [u8; 16] = message.bytes[VMCE_OPTIONS_NONCE..VMCE_OPTIONS_NONCE + 16]
            .try_into()
            .ok()?;
        if nonce == [0; 16] {
            return None;
        }
        let mut original = message.clone();
        original.put_u32(0, OPTIONS_KIND);
        Some((Self::decode(&original)?, nonce))
    }
}

#[cfg(test)]
mod tests {
    use super::super::cooperative::{CooperativeAiWords, CooperativeHumanWords};
    use super::*;

    #[test]
    fn vmce_result_record_round_trips_and_rejects_an_absent_digest() {
        let record = VmceResultRecord {
            nonce: [3; 16],
            entries: [4, 1, 0xff, 0xff],
            confirmed: true,
            stage_digest: [7; 32],
        };
        let message = record.encode("Host").unwrap();
        assert_eq!(message.u32(0), RESULT_KIND);
        assert_eq!(VmceResultRecord::decode(&message), Some(record));
        // The pre-digest record left this range zeroed. Such a packet is not
        // a result any peer may confirm, so it fails before the send path and
        // again on decode.
        let mut stale = record;
        stale.stage_digest = [0; 32];
        assert!(stale.encode("Host").is_err());
        let mut bytes = record.encode("Host").unwrap().into_bytes();
        bytes[SIGNAL_LEN + 21..SIGNAL_LEN + 53].fill(0);
        assert_eq!(
            VmceResultRecord::decode(&LanMessage::decode(&bytes).unwrap()),
            None
        );
    }

    #[test]
    fn vmce_stage_record_round_trips_and_rejects_malformed_payloads() {
        let record = VmceStageRecord {
            map_file: "VeraCampaign/mission02.map".into(),
            source_sha256: "a".repeat(64),
            effective_digest: "b".repeat(64),
            config_sha256: "c".repeat(64),
        };
        let message = record.encode("Host").unwrap();
        assert_eq!(message.u32(0), STAGE_KIND);
        assert_eq!(VmceStageRecord::decode(&message), Some(record.clone()));

        // A map name past the record's capacity, and a digest that is not a
        // SHA-256 hex string, are refused before the packet is sent.
        let mut too_long = record.clone();
        too_long.map_file = "m".repeat(STAGE_MAP_CAPACITY + 1);
        assert!(too_long.encode("Host").is_err());
        // Regression: the old 128-byte record silently dropped legal
        // `VMCE/<series>/<entry>` identities. A capacity-sized path now
        // round-trips; anything longer is rejected so the send path can
        // fail closed instead of skipping the agreement.
        let prefix = "VMCE/";
        let suffix = "/stage01/all01umd.map";
        let fill = STAGE_MAP_CAPACITY - prefix.len() - suffix.len();
        let mut boundary = record.clone();
        boundary.map_file = format!("{prefix}{}{suffix}", "s".repeat(fill));
        assert_eq!(boundary.map_file.len(), STAGE_MAP_CAPACITY);
        let message = boundary.encode("Host").unwrap();
        assert_eq!(VmceStageRecord::decode(&message), Some(boundary));
        let mut bad_digest = record;
        bad_digest.source_sha256 = String::from("not-a-digest");
        assert!(bad_digest.encode("Host").is_err());
        // A record of another round kind is never read as a stage identity.
        let options = CooperativeOptionsRecord {
            difficulty: 1,
            humans: [CooperativeHumanWords {
                start: -1,
                country: -1,
                color: -1,
            }; 8],
            ai: [CooperativeAiWords {
                country: -1,
                color: -1,
                difficulty: 0,
                start: -2,
                team: -2,
            }; 8],
        };
        assert_eq!(
            VmceStageRecord::decode(&options.encode("Host").unwrap()),
            None
        );
    }
}
