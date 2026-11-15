//! Opt-in remote-client packet metadata. No packet bodies, addresses, profile
//! names, chat, keys, cookies, or disconnect components enter this ring.
use crate::{Direction, State};
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
const HISTORY: usize = 64;

#[derive(Debug)]
struct Metadata {
    time_ms: u128,
    direction: Direction,
    state: State,
    protocol: i32,
    id: i32,
    name: &'static str,
    payload: usize,
    frame: Option<usize>,
}

pub(crate) struct Trace {
    session: String,
    wire_protocol: i32,
    state: State,
    history: VecDeque<Metadata>,
    dumps: usize,
}
impl Trace {
    pub(crate) fn new(protocol: i32) -> Self {
        Self {
            session: format!(
                "{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
            wire_protocol: protocol,
            state: State::Handshake,
            history: VecDeque::with_capacity(HISTORY + 1),
            dumps: 0,
        }
    }
    pub(crate) fn packet(&mut self, direction: Direction, id: i32, payload: usize, frame: usize) {
        self.record(direction, self.wire_protocol, id, payload, Some(frame));
    }
    fn record(
        &mut self,
        direction: Direction,
        protocol: i32,
        id: i32,
        payload: usize,
        frame: Option<usize>,
    ) {
        if self.history.len() == HISTORY + 1 {
            self.history.pop_front();
        }
        self.history.push_back(Metadata {
            time_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            direction,
            state: self.state,
            protocol,
            id,
            name: if protocol == crate::PROTOCOL_VERSION {
                crate::ids::name(self.state, direction, id).unwrap_or("unknown")
            } else {
                "older-wire-id"
            },
            payload,
            frame,
        });
    }
    pub(crate) fn outgoing(&mut self, id: i32, payload: &[u8]) {
        // Encoder-side metadata; frame length is filled after compression.
        // With an adapter, the payload/id remain canonical, not wire values.
        self.record(
            Direction::Serverbound,
            crate::PROTOCOL_VERSION,
            id,
            payload.len(),
            None,
        );
        self.state = match (self.state, id) {
            (State::Handshake, 0) => {
                let mut r = crate::buf::Reader::new(payload);
                let intent = (|| -> crate::Result<i32> {
                    r.varint()?;
                    r.string(255)?;
                    r.u16()?;
                    r.varint()
                })();
                if intent == Ok(1) {
                    State::Status
                } else {
                    State::Login
                }
            }
            (State::Login, crate::ids::login::serverbound::LOGIN_ACKNOWLEDGED) => {
                State::Configuration
            }
            (
                State::Configuration,
                crate::ids::configuration::serverbound::FINISH_CONFIGURATION,
            ) => State::Play,
            (State::Play, crate::ids::play::serverbound::CONFIGURATION_ACKNOWLEDGED) => {
                State::Configuration
            }
            _ => self.state,
        };
    }
    pub(crate) fn dump(&mut self, kind: &'static str) {
        // At most four dumps (65 metadata entries each) per connection.
        if self.dumps == 4 {
            return;
        }
        self.dumps += 1;
        log::warn!(
            "packet-diagnostics session={} event={kind} wire_protocol={} state={:?}",
            self.session,
            self.wire_protocol,
            self.state
        );
        for entry in &self.history {
            log::warn!(
                "packet-diagnostics session={} timestamp_ms={} direction={:?} state={:?} protocol={} id={:#x} name={} payload_bytes={} frame_bytes={:?}",
                self.session,
                entry.time_ms,
                entry.direction,
                entry.state,
                entry.protocol,
                entry.id,
                entry.name,
                entry.payload,
                entry.frame
            );
        }
    }
}

// Decoder messages may contain server-controlled strings (e.g. NBT names).
// Only fixed messages and numeric suffixes from audited errors may be logged.
fn safe_error(error: &crate::Error) -> &str {
    let message = error.0.as_str();
    if [
        "VarInt too big",
        "VarLong too big",
        "invalid resource identifier",
        "invalid sound source",
        "VecDelta with too many steps",
        "Server sent an impossible command tree",
        "empty merchant result",
        "inline registry values are not supported",
    ]
    .contains(&message)
    {
        return message;
    }
    if message.len() <= 160 {
        let numeric = |v: &str| v.parse::<i64>().is_ok();
        for prefix in [
            "negative count ",
            "unknown argument type ",
            "unknown number format ",
            "unknown boss event operation ",
            "position source type ",
            "waypoint operation ",
            "waypoint type ",
        ] {
            if message.strip_prefix(prefix).is_some_and(numeric) {
                return message;
            }
        }
        if let Some(rest) = message.strip_prefix("needed ")
            && let Some((needed, left)) = rest.split_once(" bytes, ")
            && let Some(left) = left.strip_suffix(" left")
            && numeric(needed)
            && numeric(left)
        {
            return message;
        }
        if let Some(rest) = message.strip_prefix("string longer than ")
            && let Some(count) = rest.strip_suffix(" chars")
            && numeric(count)
        {
            return message;
        }
    }
    "decoder rejected field (text omitted for privacy)"
}
impl Trace {
    pub(crate) fn decode_error(
        &mut self,
        id: i32,
        error: &crate::Error,
        offset: usize,
        remaining: usize,
    ) {
        if self.dumps < 4 {
            log::warn!(
                "packet-diagnostics session={} decoded_protocol={} id={:#x} name={} decoder_offset={} remaining_bytes={} error={}",
                self.session,
                crate::PROTOCOL_VERSION,
                id,
                crate::ids::name(self.state, Direction::Clientbound, id).unwrap_or("unknown"),
                offset,
                remaining,
                safe_error(error)
            );
        }
        self.dump("local packet decode failure");
    }
    pub(crate) fn frame_error(&mut self, error: &crate::Error, len: usize) {
        if self.dumps < 4 {
            log::warn!(
                "packet-diagnostics session={} frame_bytes={} error={} decoder_offset=unavailable (frame rejected before packet decoder)",
                self.session,
                len,
                safe_error(error)
            );
        }
        self.dump("local frame decode failure");
    }
    pub(crate) fn outgoing_frame(&mut self, len: usize) {
        if let Some(entry) = self
            .history
            .iter_mut()
            .rev()
            .find(|e| e.direction == Direction::Serverbound)
        {
            entry.frame = Some(len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_is_bounded_and_state_changes_follow_acknowledgements() {
        let mut t = Trace::new(crate::PROTOCOL_VERSION);
        t.state = State::Login;
        t.outgoing(crate::ids::login::serverbound::LOGIN_ACKNOWLEDGED, &[]);
        assert_eq!(t.state, State::Configuration);
        t.outgoing(
            crate::ids::configuration::serverbound::FINISH_CONFIGURATION,
            &[],
        );
        assert_eq!(t.state, State::Play);
        for _ in 0..100 {
            t.packet(Direction::Clientbound, 0, 0, 1);
        }
        assert_eq!(t.history.len(), 65);
        t.outgoing(
            crate::ids::play::serverbound::CONFIGURATION_ACKNOWLEDGED,
            &[],
        );
        assert_eq!(t.state, State::Configuration);
    }
    #[test]
    fn diagnostics_do_not_include_arbitrary_decoder_text() {
        assert_eq!(
            safe_error(&crate::Error::new("needed 12 bytes, 3 left")),
            "needed 12 bytes, 3 left"
        );
        assert_eq!(
            safe_error(&crate::Error::new("NBT: private chat or credential")),
            "decoder rejected field (text omitted for privacy)"
        );
    }
    #[test]
    fn decoder_reports_cursor_for_truncated_and_oversized_packets() {
        use crate::packets::play::Clientbound;
        let id = crate::ids::play::clientbound::PLAYER_INFO_UPDATE;
        let (_, offset, remaining) = Clientbound::decode_at(id, &[1, 1, 0]).unwrap_err();
        assert_eq!((offset, remaining), (2, 1));
        let id = crate::ids::play::clientbound::BUNDLE_DELIMITER;
        let (_, offset, remaining) = Clientbound::decode_at(id, &[9]).unwrap_err();
        assert_eq!((offset, remaining), (0, 1));
    }
}
