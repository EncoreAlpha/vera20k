//! The LAN Cooperative result dialog's records inside the global connection
//! envelope: BC6's loop `0x0046FC00` and `Dialog0xBC6__DlgProc` `0x0046DFD0`
//! send them with ack through `0x00541260` on the global manager `0xA8E9C0`.
//! The signals are the kind and the sender's name, 0x2F bytes; the master's
//! options `0x32` (`0x0046D4A0`) is 455. The round state and the slot values
//! belong to their app owners; this module only encodes and reads fields.

use super::{LAN_MESSAGE_LEN, LanMessage, LanMessageError, LanMessageKind};

/// VMCE-only new lobby identity, delivered reliably over the previous match's
/// scope before Go. The revision starts at zero for the normal setup owner.
#[cfg(feature = "vmce")]
pub const INTERMISSION_KIND: u32 = 0x3B;

/// A VMCE terminal-result agreement, scoped to this result round. Each byte
/// is either an empty raw slot (0xff), or role 0..3 plus bit 2 for victory.
#[cfg(feature = "vmce")]
pub const RESULT_KIND: u32 = 0x3C;

/// The result record's fields end before the LAN message's payload does, so
/// the digest never truncates on the wire.
#[cfg(feature = "vmce")]
const _: () = assert!(SIGNAL_LEN + 21 + 32 <= LAN_MESSAGE_LEN);

#[cfg(feature = "vmce")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmceResultRecord {
    pub nonce: [u8; 16],
    pub entries: [u8; 4],
    pub confirmed: bool,
    /// Digest of this round's resolved continuation, bound to the result nonce.
    pub stage_digest: [u8; 32],
}

#[cfg(feature = "vmce")]
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

#[cfg(feature = "vmce")]
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

#[cfg(feature = "vmce")]
pub fn decode_intermission_epoch(message: &LanMessage) -> Option<[u8; 16]> {
    if message.u32(0) != INTERMISSION_KIND {
        return None;
    }
    let epoch: [u8; 16] = message.bytes[SIGNAL_LEN..SIGNAL_LEN + 16].try_into().ok()?;
    (epoch != [0; 16]).then_some(epoch)
}

#[cfg(all(test, feature = "vmce"))]
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

pub const SIGNAL_LEN: usize = 0x2f;
pub const OPTIONS_KIND: u32 = 0x32;
const DIFFICULTY_OFFSET: usize = 0x2f;
const HUMAN_START_OFFSET: usize = 0x30;
const HUMAN_COUNTRY_OFFSET: usize = 0x50;
const HUMAN_COLOR_OFFSET: usize = 0x70;
const AI_COUNTRY_OFFSET: usize = 0x90;
const AI_COLOR_OFFSET: usize = 0xb0;
const AI_DIFFICULTY_OFFSET: usize = 0xd0;
const AI_START_OFFSET: usize = 0xf0;
const AI_TEAM_OFFSET: usize = 0x110;
const ROWS: usize = 8;

/// A header-only BC6 record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CooperativeSignal {
    /// `0x23`, a guest's ack of the begin (`0x00470896`).
    Ack,
    /// `0x24`, Quit (`0x0046E2C6`).
    Left,
    /// `0x25`, not ready (`0x0046FED6`).
    Waiting,
    /// `0x26`, ready (`0x0046FDC9`).
    Ready,
    /// `0x31`, the master's go (`0x00470A8D`, `0x00470B2C`).
    Go,
    /// `0x34`, the master's begin (`0x0046FF9C`, `0x00470244`).
    Begin,
    /// `0x37`, VERA-only: a guest's next stage differs from the host's, so
    /// both peers leave the round for the lobby. Stock BC6 has no such
    /// record; the native begin carries no stage identity at all.
    #[cfg(feature = "vmce")]
    StageRejected,
    /// `0x38`, VERA-only: this peer resolved no next stage (defeat or the
    /// campaign's last mission). Stock BC6 has no such record either.
    #[cfg(feature = "vmce")]
    StageAbsent,
    /// `0x39`, VERA-only: a guest's one retry vote on the BC6 defeat page.
    /// Stock BC6 has no such record; the native page's Continue is local.
    #[cfg(feature = "vmce")]
    RetryVote,
}

impl CooperativeSignal {
    pub const fn kind(self) -> u32 {
        match self {
            Self::Ack => 0x23,
            Self::Left => 0x24,
            Self::Waiting => 0x25,
            Self::Ready => 0x26,
            Self::Go => 0x31,
            Self::Begin => 0x34,
            #[cfg(feature = "vmce")]
            Self::StageRejected => 0x37,
            #[cfg(feature = "vmce")]
            Self::StageAbsent => 0x38,
            #[cfg(feature = "vmce")]
            Self::RetryVote => 0x39,
        }
    }

    /// The kind dword, then the narrow name from `+0x04`.
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
        Some(
            match u32::from_le_bytes(bytes[..4].try_into().expect("kind")) {
                0x23 => Self::Ack,
                0x24 => Self::Left,
                0x25 => Self::Waiting,
                0x26 => Self::Ready,
                0x31 => Self::Go,
                0x34 => Self::Begin,
                #[cfg(feature = "vmce")]
                0x37 => Self::StageRejected,
                #[cfg(feature = "vmce")]
                0x38 => Self::StageAbsent,
                #[cfg(feature = "vmce")]
                0x39 => Self::RetryVote,
                _ => return None,
            },
        )
    }
}

/// VERA-only record (`0x36`, not a native BC6 kind): the host's resolved
/// next stage identity, sent before the native begin so a guest whose own
/// campaign resolved a different stage can refuse before either peer loads
/// it. The digests are the hex strings the local resolvers already produce.
#[cfg(feature = "vmce")]
pub const STAGE_KIND: u32 = 0x36;
#[cfg(feature = "vmce")]
const STAGE_OFFSET: usize = 0x2f;
#[cfg(feature = "vmce")]
const STAGE_MAP_CAPACITY: usize = 0xd7;
#[cfg(feature = "vmce")]
const STAGE_SOURCE_OFFSET: usize = STAGE_OFFSET + 1 + STAGE_MAP_CAPACITY;
#[cfg(feature = "vmce")]
const STAGE_EFFECTIVE_OFFSET: usize = STAGE_SOURCE_OFFSET + 64;
#[cfg(feature = "vmce")]
const STAGE_CONFIG_OFFSET: usize = STAGE_EFFECTIVE_OFFSET + 64;
#[cfg(feature = "vmce")]
const STAGE_END: usize = STAGE_CONFIG_OFFSET + 64;
#[cfg(feature = "vmce")]
const _: () = assert!(STAGE_END <= LAN_MESSAGE_LEN);

#[cfg(feature = "vmce")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmceStageRecord {
    pub map_file: String,
    pub source_sha256: String,
    pub effective_digest: String,
    pub config_sha256: String,
}

#[cfg(feature = "vmce")]
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

#[cfg(feature = "vmce")]
fn digest_is_valid(digest: &str) -> bool {
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// VERA-only retry-load failure record (`0x3A`, not a native BC6 kind): the
/// reason a peer aborted the retry load, so both peers can show one native
/// box before returning to the LAN lobby.
#[cfg(feature = "vmce")]
pub const RETRY_LOAD_FAILED_KIND: u32 = 0x3A;
#[cfg(feature = "vmce")]
const RETRY_LOAD_FAILED_REASON_OFFSET: usize = SIGNAL_LEN - 1;

/// Why a retry load stopped. The numeric codes are wire values, not CSF keys.
#[cfg(feature = "vmce")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryLoadFailureReason {
    MissingMap = 0,
    ConfigMismatch = 1,
    IdentityMismatch = 2,
    LoadFailed = 3,
}

#[cfg(feature = "vmce")]
impl RetryLoadFailureReason {
    pub const fn code(self) -> u8 {
        self as u8
    }

    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => Self::MissingMap,
            1 => Self::ConfigMismatch,
            2 => Self::IdentityMismatch,
            3 => Self::LoadFailed,
            _ => return None,
        })
    }
}

#[cfg(feature = "vmce")]
pub fn encode_retry_load_failure(
    name: &str,
    reason: RetryLoadFailureReason,
) -> Result<[u8; SIGNAL_LEN], LanMessageError> {
    let mut message = LanMessage::with_name(LanMessageKind::Unknown(RETRY_LOAD_FAILED_KIND), name)?;
    message.bytes[RETRY_LOAD_FAILED_REASON_OFFSET] = reason.code();
    Ok(message.as_bytes()[..SIGNAL_LEN]
        .try_into()
        .expect("retry failure prefix"))
}

#[cfg(feature = "vmce")]
pub fn decode_retry_load_failure(bytes: &[u8]) -> Option<RetryLoadFailureReason> {
    if bytes.len() != SIGNAL_LEN
        || u32::from_le_bytes(bytes[..4].try_into().ok()?) != RETRY_LOAD_FAILED_KIND
    {
        return None;
    }
    RetryLoadFailureReason::from_code(bytes[RETRY_LOAD_FAILED_REASON_OFFSET])
}

/// One raw slot's human words: `-1` in all three for an empty or AI slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CooperativeHumanWords {
    pub start: i32,
    pub country: i32,
    pub color: i32,
}

/// One raw slot's AI words (`0xA8B29C`, `0xA8B2BC`, `0xA8B27C`,
/// `0xA8B2DC`, `0xA8B2FC`, the order of the log at `0x0081B320`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CooperativeAiWords {
    pub country: i32,
    pub color: i32,
    pub difficulty: i32,
    pub start: i32,
    pub team: i32,
}

/// The master's options `0x32`: the difficulty byte `+0x2F`
/// (`[0xA8EB68]`), the human start/country/colour arrays `+0x30`/`+0x50`/
/// `+0x70`, then the AI country, colour, difficulty, start and ally arrays
/// `+0x90..+0x110`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CooperativeOptionsRecord {
    pub difficulty: u8,
    pub humans: [CooperativeHumanWords; ROWS],
    pub ai: [CooperativeAiWords; ROWS],
}

impl CooperativeOptionsRecord {
    /// New records zero the native buffer's uninitialised tail.
    pub fn encode(&self, name: &str) -> Result<LanMessage, LanMessageError> {
        let mut message = LanMessage::with_name(LanMessageKind::Unknown(OPTIONS_KIND), name)?;
        message.bytes[DIFFICULTY_OFFSET] = self.difficulty;
        for (row, (human, ai)) in self.humans.iter().zip(&self.ai).enumerate() {
            let word = |base: usize| base + row * 4;
            message.put_u32(word(HUMAN_START_OFFSET), human.start as u32);
            message.put_u32(word(HUMAN_COUNTRY_OFFSET), human.country as u32);
            message.put_u32(word(HUMAN_COLOR_OFFSET), human.color as u32);
            message.put_u32(word(AI_COUNTRY_OFFSET), ai.country as u32);
            message.put_u32(word(AI_COLOR_OFFSET), ai.color as u32);
            message.put_u32(word(AI_DIFFICULTY_OFFSET), ai.difficulty as u32);
            message.put_u32(word(AI_START_OFFSET), ai.start as u32);
            message.put_u32(word(AI_TEAM_OFFSET), ai.team as u32);
        }
        Ok(message)
    }

    pub fn decode(message: &LanMessage) -> Option<Self> {
        if message.u32(0) != OPTIONS_KIND {
            return None;
        }
        let word = |base: usize, row: usize| message.u32(base + row * 4) as i32;
        Some(Self {
            difficulty: message.bytes[DIFFICULTY_OFFSET],
            humans: std::array::from_fn(|row| CooperativeHumanWords {
                start: word(HUMAN_START_OFFSET, row),
                country: word(HUMAN_COUNTRY_OFFSET, row),
                color: word(HUMAN_COLOR_OFFSET, row),
            }),
            ai: std::array::from_fn(|row| CooperativeAiWords {
                country: word(AI_COUNTRY_OFFSET, row),
                color: word(AI_COLOR_OFFSET, row),
                difficulty: word(AI_DIFFICULTY_OFFSET, row),
                start: word(AI_START_OFFSET, row),
                team: word(AI_TEAM_OFFSET, row),
            }),
        })
    }
}

const _: () = assert!(AI_TEAM_OFFSET + ROWS * 4 <= LAN_MESSAGE_LEN);

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "vmce")]
    #[test]
    fn retry_load_failure_round_trips_every_reason() {
        use super::{RetryLoadFailureReason, decode_retry_load_failure, encode_retry_load_failure};
        for reason in [
            RetryLoadFailureReason::MissingMap,
            RetryLoadFailureReason::ConfigMismatch,
            RetryLoadFailureReason::IdentityMismatch,
            RetryLoadFailureReason::LoadFailed,
        ] {
            let bytes = encode_retry_load_failure("Host", reason).unwrap();
            assert_eq!(decode_retry_load_failure(&bytes), Some(reason));
            assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 0x3A);
        }
        let mut bytes =
            encode_retry_load_failure("Host", RetryLoadFailureReason::MissingMap).unwrap();
        bytes[SIGNAL_LEN - 1] = 0x7F;
        assert_eq!(decode_retry_load_failure(&bytes), None);
        assert_eq!(decode_retry_load_failure(&bytes[..SIGNAL_LEN - 1]), None);
    }

    #[cfg(feature = "vmce")]
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
    fn signals_are_the_native_kind_and_name_prefix() {
        let bytes = CooperativeSignal::Ready.encode("Bob").unwrap();
        assert_eq!(bytes.len(), 0x2f);
        assert_eq!(&bytes[..8], &[0x26, 0, 0, 0, b'B', b'o', b'b', 0]);
        assert_eq!(
            CooperativeSignal::decode(&bytes),
            Some(CooperativeSignal::Ready)
        );
        // A full-length record is never a signal.
        assert_eq!(CooperativeSignal::decode(&[0x26; LAN_MESSAGE_LEN]), None);
    }

    #[test]
    fn options_words_sit_at_the_native_offsets() {
        let mut record = CooperativeOptionsRecord {
            difficulty: 2,
            humans: [CooperativeHumanWords {
                start: -1,
                country: -1,
                color: -1,
            }; ROWS],
            ai: [CooperativeAiWords {
                country: -1,
                color: -1,
                difficulty: 0,
                start: -2,
                team: -2,
            }; ROWS],
        };
        record.humans[1] = CooperativeHumanWords {
            start: -2,
            country: 4,
            color: 6,
        };
        record.ai[7] = CooperativeAiWords {
            country: 9,
            color: 3,
            difficulty: 1,
            start: 5,
            team: 2,
        };
        let message = record.encode("Host").unwrap();
        let bytes = message.as_bytes();
        let word =
            |offset: usize| i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        assert_eq!(word(0), 0x32);
        assert_eq!(bytes[0x2f], 2);
        assert_eq!(
            (word(0x34), word(0x54), word(0x74)),
            (-2, 4, 6),
            "slot 1's start, country and colour"
        );
        assert_eq!(
            (word(0xac), word(0xcc), word(0xec), word(0x10c), word(0x12c)),
            (9, 3, 1, 5, 2),
            "slot 7's AI country, colour, difficulty, start and ally"
        );
        assert_eq!(CooperativeOptionsRecord::decode(&message), Some(record));
    }

    #[cfg(feature = "vmce")]
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
            }; ROWS],
            ai: [CooperativeAiWords {
                country: -1,
                color: -1,
                difficulty: 0,
                start: -2,
                team: -2,
            }; ROWS],
        };
        assert_eq!(
            VmceStageRecord::decode(&options.encode("Host").unwrap()),
            None
        );
    }
}
