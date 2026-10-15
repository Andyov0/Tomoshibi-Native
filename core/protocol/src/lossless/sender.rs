//! Sending shared sound without loss. Follows web/src/live/lossless-sender.ts.
//!
//! The samples are taken off the captured sound as the platform delivered
//! them, before any encoder has seen them, in blocks of [`FRAMES`]; each block
//! is coded and queued for the people who asked for it, and for nobody else.
//! Asking is what keeps this from costing anything where it is not used: a
//! browser on an older page, or one whose owner has turned this person down,
//! never asks, and the media server is never handed a packet for it. An ask
//! lasts three times [`ASK_EVERY_MS`] and is repeated every [`ASK_EVERY_MS`],
//! so a listener who vanishes without a word stops being sent to shortly
//! after.
//!
//! The web client sends from its own promise loop; here the queue is kept and
//! the media crate drains it. It calls [`LosslessSender::next_packet`], publishes what
//! comes back (reliable, on the packet's topic, to its identities), waits for
//! that to finish, and calls again until there is nothing, one packet at a
//! time; it starts again whenever [`LosslessSender::capture`] has queued more.
//! The packet being published is no longer in the queue, as in the web client,
//! and that is what the give-up threshold counts against.
//!
//! The web client also listens for asks before the track that invites them is
//! published, because a listener that saw the publication at once asked
//! before anything was listening and the ask was lost (sound.ts,
//! publishSound). The media crate should make the sender first and publish
//! the Opus track under [`Channel::lossless`] after.

use std::collections::VecDeque;

use super::format::{pack, to_int24, Block, FRAMES};
use super::{Channel, Outgoing, ASK_EVERY_MS};
use crate::ProtocolError;

/// How long an ask lasts unrepeated.
pub const ASK_LASTS_MS: u64 = 3 * ASK_EVERY_MS;

/// How much sound may wait to be sent before this gives up.
///
/// Reliable delivery is a promise to arrive late rather than not at all, and an
/// upload too slow for the stream turns that into a queue that only grows.
/// Past a few seconds the people listening are better served by the Opus track
/// they fall back to than by a stream that will never catch up.
pub const BACKLOG_SECONDS: u64 = 3;

/// A text decoder drops a byte-order mark before it compares, so a browser
/// takes these bytes followed by "1" as a yes; so does this.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// What one chunk of capture did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Captured {
    /// Packets it added to the queue.
    pub queued: usize,
    /// Whether it gave up on the stream: the upload cannot keep up. Happens
    /// once; the sender is stopped and its queue emptied. The web client tells
    /// the person sharing, and the listeners fall back to Opus by themselves.
    pub gave_up: bool,
}

/// One lossless stream going out, of one [`Channel`].
pub struct LosslessSender {
    channel: &'static Channel,
    to_int: fn(f32) -> i32,
    /// Who asked and when, in the order they first asked: the order the web
    /// client's Map keeps, and so the order of each packet's destinations.
    wanting: Vec<(String, u64)>,
    queue: VecDeque<Outgoing>,
    stopped: bool,
    /// Whether capture is still being read. A packet that could not be made
    /// ends the reading, as an exception ends the web client's read loop.
    reading: bool,
    streams: Box<dyn FnMut() -> u32 + Send>,

    stream: u32,
    /// Counts every block made, sent or not, as the web client's does.
    seq: u64,
    rate: u32,
    channels: usize,
    block: Vec<Vec<i32>>,
    filled: usize,
    /// Where the next chunk should start, in microseconds.
    expected: Option<f64>,
    gaps: u64,
}

impl LosslessSender {
    /// A sender for `channel`, turning each float into a sample with `to_int`:
    /// [`to_int24`] for anything captured, or [`super::decoded_to_int24`] for
    /// a file's floats whose own integers can be recovered.
    pub fn new(channel: &'static Channel, to_int: fn(f32) -> i32) -> Self {
        Self {
            channel,
            to_int,
            wanting: Vec::new(),
            queue: VecDeque::new(),
            stopped: false,
            reading: true,
            streams: Box::new(rand::random::<u32>),
            stream: 0,
            seq: 0,
            rate: 0,
            channels: 0,
            block: Vec::new(),
            filled: 0,
            expected: None,
            gaps: 0,
        }
    }

    /// A sender with captured floats converted by [`to_int24`].
    pub fn captured(channel: &'static Channel) -> Self {
        Self::new(channel, to_int24)
    }

    /// Where stream ids come from; random unless a test says otherwise. One is
    /// taken at the first chunk and at every change of format.
    pub fn with_streams(mut self, streams: impl FnMut() -> u32 + Send + 'static) -> Self {
        self.streams = Box::new(streams);
        self
    }

    pub fn channel(&self) -> &'static Channel {
        self.channel
    }

    /// Chunks of capture that were missing between two that arrived.
    pub fn gaps(&self) -> u64 {
        self.gaps
    }

    /// Whether it has stopped, by [`LosslessSender::stop`] or by giving up.
    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// Packets waiting to be published.
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// A data packet arrived on `topic` from `from`. Only asks on this
    /// channel's ask topic mean anything: "1" is a yes, anything else a no.
    pub fn on_ask(&mut self, now: u64, from: &str, topic: &str, payload: &[u8]) {
        if self.stopped || topic != self.channel.ask {
            return;
        }

        let text = payload.strip_prefix(BOM).unwrap_or(payload);
        if text == b"1" {
            match self
                .wanting
                .iter_mut()
                .find(|(identity, _)| identity == from)
            {
                Some((_, at)) => *at = now,
                None => self.wanting.push((from.to_owned(), now)),
            }
        } else {
            self.wanting.retain(|(identity, _)| identity != from);
        }
    }

    /// Somebody left the room.
    pub fn on_left(&mut self, identity: &str) {
        if self.stopped {
            return;
        }
        self.wanting.retain(|(who, _)| who != identity);
    }

    /// A chunk of capture: one plane per channel, all the same length, at
    /// `rate`, starting at `timestamp` microseconds on the capture's clock.
    /// Only the first two planes are sent.
    ///
    /// `present` says whether an identity is still in the room; an ask from
    /// somebody who left without a word lapses at the next block.
    ///
    /// A change of rate or channel count is a new stream: the far end starts
    /// again rather than playing samples at the wrong rate, and the part of a
    /// block made before the change is dropped.
    ///
    /// An error is a block that could not be packed, which only a custom
    /// `to_int` can cause; reading ends there, as it does in the web client,
    /// and the listeners fall back when the packets stop.
    pub fn capture<P: AsRef<[f32]>>(
        &mut self,
        now: u64,
        rate: u32,
        timestamp: i64,
        planes: &[P],
        present: impl Fn(&str) -> bool,
    ) -> Result<Captured, ProtocolError> {
        let mut captured = Captured::default();
        if self.stopped || !self.reading {
            return Ok(captured);
        }

        let count = planes.len().min(2);
        if rate != self.rate || count != self.channels {
            self.rate = rate;
            self.channels = count;
            self.block = vec![vec![0; FRAMES]; count];
            self.filled = 0;
            self.expected = None;
            self.stream = (self.streams)();
            self.seq = 0;
        }

        let frames = planes.first().map_or(0, |plane| plane.as_ref().len());

        // Anything more than a millisecond off where the last chunk ended is
        // capture the platform did not hand over. Counted, never smoothed.
        let timestamp = timestamp as f64;
        if let Some(expected) = self.expected {
            if (timestamp - expected).abs() > 1000.0 {
                self.gaps += 1;
            }
        }
        self.expected = Some(timestamp + (frames as f64 / f64::from(rate)) * 1_000_000.0);

        let mut offset = 0;
        while offset < frames {
            let take = (FRAMES - self.filled).min(frames - offset);
            for (c, to) in self.block.iter_mut().enumerate() {
                let from = planes[c].as_ref();
                for i in 0..take {
                    // A plane shorter than the first reads as NaN, which is
                    // what the web client's typed array gives past its end.
                    let sample = from.get(offset + i).copied().unwrap_or(f32::NAN);
                    to[self.filled + i] = (self.to_int)(sample);
                }
            }
            self.filled += take;
            offset += take;

            if self.filled == FRAMES {
                let block = std::mem::replace(&mut self.block, vec![vec![0; FRAMES]; count]);
                self.filled = 0;
                match self.emit(now, block, &present) {
                    Ok(Emitted::Nothing) => {}
                    Ok(Emitted::Queued) => captured.queued += 1,
                    Ok(Emitted::GaveUp) => {
                        captured.gave_up = true;
                        break;
                    }
                    Err(error) => {
                        self.reading = false;
                        return Err(error);
                    }
                }
            }
        }

        Ok(captured)
    }

    /// The next packet to publish, if any. See the module's notes on draining.
    pub fn next_packet(&mut self) -> Option<Outgoing> {
        if self.stopped {
            return None;
        }
        self.queue.pop_front()
    }

    /// Stop sending: the queue is dropped, and asks and capture are ignored.
    pub fn stop(&mut self) {
        self.stopped = true;
        self.queue.clear();
    }

    /// Who to send the next block to: whoever has asked lately and is still here.
    fn recipients(&mut self, now: u64, present: &dyn Fn(&str) -> bool) -> Vec<String> {
        self.wanting
            .retain(|(identity, at)| now.saturating_sub(*at) <= ASK_LASTS_MS && present(identity));
        self.wanting
            .iter()
            .map(|(identity, _)| identity.clone())
            .collect()
    }

    fn emit(
        &mut self,
        now: u64,
        channels: Vec<Vec<i32>>,
        present: &dyn Fn(&str) -> bool,
    ) -> Result<Emitted, ProtocolError> {
        let seq = self.seq;
        self.seq += 1;
        let to = self.recipients(now, present);
        if to.is_empty() {
            return Ok(Emitted::Nothing);
        }

        // The sequence number is a JavaScript number in the web client,
        // written as its low 32 bits.
        let payload = pack(&Block {
            rate: self.rate,
            stream: self.stream,
            seq: seq as u32,
            channels,
        })?;
        self.queue.push_back(Outgoing {
            topic: self.channel.topic,
            to,
            payload,
        });

        // More than BACKLOG_SECONDS of packets waiting: queue.length >
        // BACKLOG_SECONDS * rate / FRAMES in the web client, in integers here.
        if self.queue.len() as u64 * FRAMES as u64 > BACKLOG_SECONDS * u64::from(self.rate) {
            self.stop();
            return Ok(Emitted::GaveUp);
        }

        Ok(Emitted::Queued)
    }
}

enum Emitted {
    Nothing,
    Queued,
    GaveUp,
}
