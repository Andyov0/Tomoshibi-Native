//! The packet and the coding inside it. Follows web/src/live/lossless.ts.
//!
//! The same idea as FLAC: a fixed polynomial predictor chosen per block, stereo
//! optionally coded as mid and side, the prediction error written in Rice codes
//! with a parameter per partition and a verbatim escape. A browser decodes
//! every packet this writes and writes every packet this decodes, so the
//! encoder's choices are ported as well as the format: a packet that decodes
//! to the right samples but differs in a bit from what the web client would
//! have written is a packet nobody tested. The fixtures under
//! tests/fixtures/lossless/ are made by the web client's own code and hold
//! this to it byte for byte.
//!
//! The web client does its arithmetic in doubles and stores into Int32Array,
//! which wraps. Here sums run in 64-bit integers, which are exact wherever the
//! doubles are (no value in this format comes near 2^53), and the stores that
//! can wrap in the browser wrap here too, so that even a corrupt packet that
//! slips past the checks decodes to the same garbage on both.

use std::borrow::Cow;

use crate::ProtocolError;

/// Samples per channel in a packet: 20 milliseconds at 48 kHz.
///
/// Short enough that the largest packet, a block of full-scale noise, is about
/// six kilobytes, which a data message carries through the media server
/// comfortably.
pub const FRAMES: usize = 960;

/// The largest and smallest 24-bit sample.
pub const TOP: i32 = 8_388_607;
pub const BOTTOM: i32 = -8_388_608;

const VERSION: u8 = 1;
const HEADER: usize = 16;

/// Partitions a channel's residual is split into, each with its own parameter.
const PARTITIONS: usize = 8;

const MAX_ORDER: usize = 4;

/// Bits a warm-up sample is written in: a side channel is 25 bits signed.
const WARMUP_BITS: u32 = 27;
const WARMUP_OFFSET: i64 = 1 << (WARMUP_BITS - 1);

/// The Rice parameter that means the values are written plainly.
const ESCAPE: u32 = 31;

/// One packet: which stream, where in it, and the samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Samples a second.
    pub rate: u32,
    /// Chosen afresh each time somebody starts, so a restart is not a continuation.
    pub stream: u32,
    /// The packet's place in its stream. Consecutive from the first.
    pub seq: u32,
    /// One vector per channel, 24-bit integers, all the same length.
    pub channels: Vec<Vec<i32>>,
}

/// A float sample, as the capture has it, as a 24-bit integer.
///
/// Exact for anything that came from a 16- or 24-bit source, whose samples
/// all have a float of their own. Rounds half up, as JavaScript's Math.round
/// does, and not away from zero as Rust's `round` does: a capture float
/// exactly between two samples below zero would otherwise go to a different
/// sample than in the browser. Clips rather than wraps past full scale; NaN
/// is 0, which is what the browser's Int32Array makes of the NaN its version
/// returns.
pub fn to_int24(sample: f32) -> i32 {
    let scaled = js_round(f64::from(sample) * 8_388_608.0);
    let clipped = if scaled > f64::from(TOP) {
        f64::from(TOP)
    } else if scaled < f64::from(BOTTOM) {
        f64::from(BOTTOM)
    } else {
        scaled
    };
    to_int32(clipped)
}

/// And back. Exact for every 24-bit integer; anything wider is rounded to the
/// nearest float, as the browser's Float32Array does.
pub fn from_int24(sample: i32) -> f32 {
    (f64::from(sample) / 8_388_608.0) as f32
}

/// How a float a decoder produced becomes the 24-bit sample sent, given the
/// bit depth of the file it came from.
///
/// Chrome turns a decoded integer into a float asymmetrically: a negative one
/// is divided by 2^(n-1) and a positive one by 2^(n-1) - 1, so both rails land
/// on exactly -1 and 1. For a 16-bit file the integer is recovered with the
/// scale Chrome used for its sign and sent shifted up eight bits; the one-scale
/// [`to_int24`] moves every positive sample a little, and three samples in four
/// of a stereo file came out changed before the web client learned this.
///
/// A 24-bit file needs nothing of the kind (the two scales differ by less than
/// a 256th of a step there), and neither does anything that was never
/// integers. A native decoder that divides both signs by 2^(n-1) is the
/// one-scale case and wants [`to_int24`] whatever the depth; this is for floats
/// that came through Chrome's conversion, or one that copies it.
pub fn decoded_to_int24(bits: u32) -> fn(f32) -> i32 {
    if bits == 16 {
        chrome16
    } else {
        to_int24
    }
}

/// The 16-bit recovery. Deliberately unclipped, like the web client's: a float
/// past full scale cannot have come from a 16-bit file, and the packet refuses
/// it rather than sending something else.
fn chrome16(sample: f32) -> i32 {
    let sample = f64::from(sample);
    let integer = if sample > 0.0 {
        js_round(sample * 32_767.0)
    } else {
        js_round(sample * 32_768.0)
    };
    to_int32(integer * 256.0)
}

/// JavaScript's Math.round: the nearest integer, and the one towards positive
/// infinity on a tie. `x - floor(x)` is exact in binary floating point, so the
/// comparison with one half is too.
fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// ECMAScript's ToInt32, which is what storing a number into an Int32Array
/// does: truncate, wrap modulo 2^32, and nought for NaN and the infinities.
fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    x.trunc().rem_euclid(4_294_967_296.0) as u32 as i32
}

fn invalid(why: impl Into<String>) -> ProtocolError {
    ProtocolError::Invalid {
        message: why.into(),
    }
}

/// A packet that is not one of ours, or not whole. The wording is the web
/// client's `Malformed`, so a log from either end reads the same.
fn malformed(why: impl std::fmt::Display) -> ProtocolError {
    ProtocolError::Unexpected {
        message: format!("not a lossless packet: {why}"),
    }
}

/// A block, as bytes.
///
/// Refuses, as the web client throws a RangeError for, anything but one or two
/// channels of equal length between 1 and 65535 frames, and any sample that is
/// not 24-bit, rather than wrapping it into one that is.
pub fn pack(block: &Block) -> Result<Vec<u8>, ProtocolError> {
    let count = block.channels.len();
    let frames = block.channels.first().map_or(0, Vec::len);

    if !(1..=2).contains(&count) {
        return Err(invalid("one or two channels"));
    }
    if !(1..=0xffff).contains(&frames) {
        return Err(invalid("between 1 and 65535 frames"));
    }
    for channel in &block.channels {
        if channel.len() != frames {
            return Err(invalid("channels of different lengths"));
        }
        if let Some(sample) = channel
            .iter()
            .find(|&&sample| !(BOTTOM..=TOP).contains(&sample))
        {
            return Err(invalid(format!("sample {sample} is not 24-bit")));
        }
    }

    let mut bits = BitWriter::with_capacity(HEADER + frames * count * 4);
    encode_channels(&mut bits, &block.channels);
    let body = bits.finish();

    let mut out = Vec::with_capacity(HEADER + body.len());
    out.push(VERSION);
    out.push(count as u8);
    out.extend_from_slice(&(frames as u16).to_le_bytes());
    out.extend_from_slice(&block.rate.to_le_bytes());
    out.extend_from_slice(&block.stream.to_le_bytes());
    out.extend_from_slice(&block.seq.to_le_bytes());
    out.extend_from_slice(&body);

    Ok(out)
}

/// Bytes, as a block. Any error is the web client's `Malformed`: the receiver
/// drops such a packet and carries on.
///
/// Bytes past the end of the coded samples are ignored, as the web client
/// ignores them.
pub fn unpack(bytes: &[u8]) -> Result<Block, ProtocolError> {
    if bytes.len() < HEADER {
        return Err(malformed("shorter than its header"));
    }
    if bytes[0] != VERSION {
        return Err(malformed(format_args!("version {}", bytes[0])));
    }

    let count = usize::from(bytes[1]);
    let frames = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
    if !(1..=2).contains(&count) {
        return Err(malformed(format_args!("{count} channels")));
    }
    if frames < 1 {
        return Err(malformed("no frames"));
    }

    let word =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let channels = decode_channels(&mut BitReader::new(&bytes[HEADER..]), count, frames)?;

    Ok(Block {
        rate: word(4),
        stream: word(8),
        seq: word(12),
        channels,
    })
}

/*
 * The coding.
 *
 * Per packet: for two channels, one bit saying whether they are coded as left
 * and right or as mid and side. Then per coded channel: five bits of wasted low
 * bits, three bits of predictor order, that many warm-up samples verbatim, and
 * the residual in partitions, each opening with five bits of Rice parameter --
 * or 31, then five bits of width, for values written plainly.
 *
 * The wasted bits are the low bits every sample in the channel has clear: a
 * 16-bit source in this 24-bit container leaves the bottom eight empty in
 * every sample, which is a third of the stream for most music.
 */

fn encode_channels(bits: &mut BitWriter, channels: &[Vec<i32>]) {
    if let [mono] = channels {
        encode_channel(bits, mono);
        return;
    }

    let (left, right) = (&channels[0], &channels[1]);
    // Both are 24-bit, so neither the sum nor the difference leaves an i32;
    // the shift floors, as JavaScript's `>>` does.
    let mid: Vec<i32> = left
        .iter()
        .zip(right)
        .map(|(&l, &r)| (l + r) >> 1)
        .collect();
    let side: Vec<i32> = left.iter().zip(right).map(|(&l, &r)| l - r).collect();

    // Whichever pair predicts more cheaply, judged on the samples before their
    // wasted bits come off, as the web client judges it. A tie goes to left
    // and right.
    let apart = best_order(left).1 + best_order(right).1;
    let together = best_order(&mid).1 + best_order(&side).1;

    if together < apart {
        bits.write(1, 1);
        encode_channel(bits, &mid);
        encode_channel(bits, &side);
    } else {
        bits.write(0, 1);
        encode_channel(bits, left);
        encode_channel(bits, right);
    }
}

fn decode_channels(
    bits: &mut BitReader,
    count: usize,
    frames: usize,
) -> Result<Vec<Vec<i32>>, ProtocolError> {
    if count == 1 {
        return Ok(vec![decode_channel(bits, frames)?]);
    }

    let joint = bits.read(1)? == 1;
    let a = decode_channel(bits, frames)?;
    let b = decode_channel(bits, frames)?;
    if !joint {
        return Ok(vec![a, b]);
    }

    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for (&mid, &side) in a.iter().zip(&b) {
        let side64 = i64::from(side);
        // The bit the halving dropped is the low bit of the side: the sum and
        // the difference of two integers are both odd or both even.
        let doubled = i64::from(mid) * 2 + i64::from(side & 1);
        // Truncating to 32 bits is the Int32Array store the web client makes;
        // it only matters for a corrupt packet, which it wraps the same way.
        left.push(((doubled + side64) / 2) as i32);
        right.push(((doubled - side64) / 2) as i32);
    }

    Ok(vec![left, right])
}

/// The residual of one fixed predictor at one position.
fn residual(x: &[i32], i: usize, order: usize) -> i64 {
    let at = |back: usize| i64::from(x[i - back]);
    match order {
        0 => at(0),
        1 => at(0) - at(1),
        2 => at(0) - 2 * at(1) + at(2),
        3 => at(0) - 3 * at(1) + 3 * at(2) - at(3),
        _ => at(0) - 4 * at(1) + 6 * at(2) - 4 * at(3) + at(4),
    }
}

/// What the residual is added to on the way back. The same polynomials.
fn prediction(x: &[i32], i: usize, order: usize) -> i64 {
    let at = |back: usize| i64::from(x[i - back]);
    match order {
        0 => 0,
        1 => at(1),
        2 => 2 * at(1) - at(2),
        3 => 3 * at(1) - 3 * at(2) + at(3),
        _ => 4 * at(1) - 6 * at(2) + 4 * at(3) - at(4),
    }
}

/// The order whose residual is smallest in sum, and that sum, as FLAC chooses
/// it. The lowest order wins a tie.
fn best_order(x: &[i32]) -> (usize, i64) {
    let top = MAX_ORDER.min(x.len().saturating_sub(1));
    let mut best = (0, i64::MAX);

    for order in 0..=top {
        let cost: i64 = (order..x.len()).map(|i| residual(x, i, order).abs()).sum();
        if cost < best.1 {
            best = (order, cost);
        }
    }

    best
}

/// Signed to unsigned, small magnitudes to small numbers.
fn zigzag(value: i64) -> u64 {
    if value >= 0 {
        (value as u64) * 2
    } else {
        ((-value) as u64) * 2 - 1
    }
}

fn unzigzag(value: u64) -> i64 {
    if value & 1 == 0 {
        (value / 2) as i64
    } else {
        -(((value - 1) / 2) as i64) - 1
    }
}

/// Where each partition starts and ends, the first shortened by the warm-up.
fn partitions(frames: usize, order: usize) -> impl Iterator<Item = (usize, usize)> {
    let count = if frames >= PARTITIONS * 8 {
        PARTITIONS
    } else {
        1
    };
    let size = frames / count;

    (0..count).map(move |p| {
        let start = if p == 0 { order } else { p * size };
        let end = if p == count - 1 {
            frames
        } else {
            (p + 1) * size
        };
        (start, end)
    })
}

/// Low bits clear in every sample, at most 24; none for silence.
fn wasted_bits(x: &[i32]) -> u32 {
    let all = x.iter().fold(0, |all, &sample| all | sample);
    if all == 0 {
        0
    } else {
        all.trailing_zeros().min(24)
    }
}

fn encode_channel(bits: &mut BitWriter, samples: &[i32]) {
    let shift = wasted_bits(samples);
    bits.write(u64::from(shift), 5);
    // Exact: the bits shifted out are all clear.
    let x: Cow<[i32]> = if shift == 0 {
        Cow::Borrowed(samples)
    } else {
        Cow::Owned(samples.iter().map(|&sample| sample >> shift).collect())
    };

    let (order, _) = best_order(&x);
    bits.write(order as u64, 3);
    for &sample in &x[..order] {
        bits.write((i64::from(sample) + WARMUP_OFFSET) as u64, WARMUP_BITS);
    }

    for (start, end) in partitions(x.len(), order) {
        let values: Vec<u64> = (start..end)
            .map(|i| zigzag(residual(&x, i, order)))
            .collect();
        let sum: u64 = values.iter().sum();
        let largest = values.iter().copied().max().unwrap_or(0);
        let len = values.len() as u64;

        // The parameter near log2 of the mean, tried a step either side and
        // against writing the values plainly; whichever is shortest, the
        // plain form keeping a tie. Plain wins on noise near full scale, where
        // the unary bit Rice spends on every value is pure overhead.
        //
        // The web client takes floor(log2(x)) in floating point. Here it is
        // the integer bit length, which is the same for every value this
        // coding produces: checked in V8 for every partition length up to
        // 65535 and every sum up to 2^31 times it, around each power of two,
        // where the two could part. They first differ near 2^49.
        let width = if largest == 0 {
            0
        } else {
            64 - largest.leading_zeros()
        };
        let mut choice = (ESCAPE, 5 + len * u64::from(width));

        let guess = if len == 0 || sum < len {
            0
        } else {
            (sum / len).ilog2()
        };
        for k in guess.saturating_sub(1)..=(guess + 1).min(30) {
            let cost: u64 = values.iter().map(|&u| (u >> k) + 1 + u64::from(k)).sum();
            if cost < choice.1 {
                choice = (k, cost);
            }
        }

        if choice.0 == ESCAPE {
            bits.write(u64::from(ESCAPE), 5);
            bits.write(u64::from(width), 5);
            for &u in &values {
                bits.write(u, width);
            }
        } else {
            bits.write(u64::from(choice.0), 5);
            for &u in &values {
                bits.rice(u, choice.0);
            }
        }
    }
}

fn decode_channel(bits: &mut BitReader, frames: usize) -> Result<Vec<i32>, ProtocolError> {
    let shift = bits.read(5)? as u32;
    if shift > 24 {
        return Err(malformed(format_args!("{shift} wasted bits")));
    }

    let order = bits.read(3)? as usize;
    if order > MAX_ORDER || order > frames {
        return Err(malformed(format_args!("predictor order {order}")));
    }

    let mut x = vec![0i32; frames];
    // A warm-up is not range-checked, in the web client either; it fits an
    // i32 whatever the bits say.
    for sample in &mut x[..order] {
        *sample = (bits.read(WARMUP_BITS)? as i64 - WARMUP_OFFSET) as i32;
    }

    const LIMIT: i64 = 1 << 25;
    for (start, end) in partitions(frames, order) {
        let k = bits.read(5)? as u32;
        let width = if k == ESCAPE { bits.read(5)? as u32 } else { 0 };

        for i in start..end {
            let u = if k == ESCAPE {
                bits.read(width)?
            } else {
                bits.unrice(k)?
            };
            let value = unzigzag(u) + prediction(&x, i, order);
            if !(-LIMIT..=LIMIT).contains(&value) {
                return Err(malformed("a sample out of range"));
            }
            x[i] = value as i32;
        }
    }

    if shift > 0 {
        // The web client multiplies and stores into an Int32Array, which
        // wraps; a valid packet never gets near it.
        for sample in &mut x {
            *sample = (i64::from(*sample) << shift) as i32;
        }
    }

    Ok(x)
}

/*
 * Bits, most significant first.
 */

struct BitWriter {
    bytes: Vec<u8>,
    pending: u64,
    count: u32,
}

impl BitWriter {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity.max(64)),
            pending: 0,
            count: 0,
        }
    }

    /// Write the low `width` bits of a value that fits in them; up to 32.
    fn write(&mut self, value: u64, width: u32) {
        if width == 0 {
            return;
        }
        debug_assert!(
            width <= 32 && value >> width == 0,
            "{value} does not fit in {width} bits"
        );
        self.pending = (self.pending << width) | value;
        self.count += width;

        while self.count >= 8 {
            self.count -= 8;
            self.bytes.push((self.pending >> self.count) as u8);
            self.pending &= (1 << self.count) - 1;
        }
    }

    /// A Rice code: the quotient in unary, ones-terminated, then k bits.
    fn rice(&mut self, value: u64, k: u32) {
        let mut quotient = value >> k;
        while quotient >= 32 {
            self.write(0, 32);
            quotient -= 32;
        }
        self.write(1, quotient as u32 + 1);
        if k > 0 {
            self.write(value & ((1 << k) - 1), k);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            self.bytes.push((self.pending << (8 - self.count)) as u8);
        }
        self.bytes
    }
}

struct BitReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn bit(&mut self) -> Result<u64, ProtocolError> {
        let byte = self
            .bytes
            .get(self.position >> 3)
            .ok_or_else(|| malformed("cut short"))?;
        let value = (byte >> (7 - (self.position & 7))) & 1;
        self.position += 1;
        Ok(u64::from(value))
    }

    fn read(&mut self, width: u32) -> Result<u64, ProtocolError> {
        let mut value = 0;
        for _ in 0..width {
            value = value * 2 + self.bit()?;
        }
        Ok(value)
    }

    fn unrice(&mut self, k: u32) -> Result<u64, ProtocolError> {
        let mut quotient: u64 = 0;
        while self.bit()? == 0 {
            quotient += 1;
            // No residual this coding writes is anywhere near this long; a run
            // of zeros past it is a packet that is not one. It also keeps the
            // value below 2^57, inside the integers here.
            if quotient > 1 << 26 {
                return Err(malformed("a runaway code"));
            }
        }

        Ok((quotient << k) + self.read(k)?)
    }
}
