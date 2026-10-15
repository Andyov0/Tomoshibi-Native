//! The lossless format: web/src/live/lossless.test.ts, case for case.
//!
//! The one property that matters is that what comes out is what went in, every
//! sample, for every kind of signal, and the kinds that break a coder like this
//! are not the musical ones. They are the shapes that push a branch to its
//! edge: full-scale noise that does not compress, a square wave at both rails
//! whose fourth-order residual is the largest possible, a single spike in
//! silence that only the verbatim escape writes sensibly, blocks too short to
//! have partitions, and a mono stream. Whether the bytes are the browser's is
//! lossless_fixtures.rs; this is whether the format keeps its promises.

use tomoshibi_protocol::lossless::{
    decoded_to_int24, from_int24, pack, to_int24, unpack, Block, BOTTOM, FRAMES, TOP,
};
use tomoshibi_protocol::ProtocolError;

/// The web tests' seeded generator, so a failure is the same failure on the next run.
fn random(seed: u32) -> impl FnMut() -> f64 {
    let mut state = seed;
    move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(state) / 2f64.powi(32)
    }
}

fn block(channels: Vec<Vec<i32>>, seq: u32) -> Block {
    Block {
        rate: 48_000,
        stream: 0xdead_beef,
        seq,
        channels,
    }
}

fn expect_same(input: &Block) -> Block {
    let output = unpack(&pack(input).expect("packs")).expect("unpacks");
    assert_eq!(output.rate, input.rate);
    assert_eq!(output.stream, input.stream);
    assert_eq!(output.seq, input.seq);
    assert_eq!(output.channels.len(), input.channels.len());
    for (out, inp) in output.channels.iter().zip(&input.channels) {
        assert!(out == inp, "a channel came back different");
    }
    output
}

fn frames(n: usize, make: impl FnMut(usize) -> i32) -> Vec<i32> {
    (0..n).map(make).collect()
}

fn round(x: f64) -> i32 {
    x.round() as i32
}

fn full_scale(next: &mut impl FnMut() -> f64) -> i32 {
    (next() * f64::from(TOP - BOTTOM + 1)).floor() as i32 + BOTTOM
}

#[test]
fn silence() {
    expect_same(&block(vec![vec![0; FRAMES], vec![0; FRAMES]], 7));
}

#[test]
fn music_like_stereo_the_channels_mostly_alike() {
    let left = frames(FRAMES, |i| {
        round(3_000_000.0 * (i as f64 / 7.0).sin() + 800_000.0 * (i as f64 / 1.3).sin())
    });
    let right = left
        .iter()
        .enumerate()
        .map(|(i, v)| v + round(40_000.0 * (i as f64 / 3.0).sin()))
        .collect();
    expect_same(&block(vec![left, right], 7));
}

#[test]
fn channels_doing_different_things() {
    expect_same(&block(
        vec![
            frames(FRAMES, |i| round(f64::from(TOP) * (i as f64 / 5.0).sin())),
            frames(FRAMES, |i| {
                round(f64::from(BOTTOM) * (i as f64 / 11.0).cos())
            }),
        ],
        7,
    ));
}

#[test]
fn full_scale_noise_which_does_not_compress() {
    let mut next = random(1);
    let left = frames(FRAMES, |_| full_scale(&mut next));
    let right = frames(FRAMES, |_| full_scale(&mut next));
    expect_same(&block(vec![left, right], 7));
}

#[test]
fn a_square_wave_at_both_rails_the_largest_residual_there_is() {
    let rail = |i: usize| if i % 2 == 1 { TOP } else { BOTTOM };
    expect_same(&block(
        vec![frames(FRAMES, rail), frames(FRAMES, |i| rail(i + 1))],
        7,
    ));
}

#[test]
fn one_spike_in_silence_in_a_packet_still_small() {
    let spike = block(
        vec![
            frames(FRAMES, |i| if i == 500 { TOP } else { 0 }),
            frames(FRAMES, |i| if i == 501 { BOTTOM } else { 0 }),
        ],
        7,
    );
    expect_same(&spike);
    assert!(pack(&spike).unwrap().len() < 1024);
}

#[test]
fn mono() {
    let mut next = random(2);
    expect_same(&block(
        vec![frames(FRAMES, |_| {
            (next() * 2_000_000.0).floor() as i32 - 1_000_000
        })],
        7,
    ));
}

#[test]
fn blocks_too_short_for_partitions_down_to_one_frame() {
    let mut next = random(3);
    for n in [1, 2, 3, 4, 5, 9, 63, 64, 65, 961] {
        let left = frames(n, |_| full_scale(&mut next));
        let right = frames(n, |_| full_scale(&mut next));
        expect_same(&block(vec![left, right], 7));
    }
}

#[test]
fn a_thousand_random_blocks_of_every_character() {
    let mut next = random(4);
    for round_ in 0..1000u32 {
        let scale = 2f64.powi((next() * 24.0).floor() as i32);
        let shape = (next() * 3.0).floor() as u32;
        // Drawn before the samples, as the web test draws them.
        let n = 1 + (next() * 1200.0).floor() as usize;
        let stereo = next() < 0.8;
        let mut make = |i: usize| {
            let v = match shape {
                0 => ((next() - 0.5) * scale).floor(),
                1 => (scale * (i as f64 / (1.0 + next() * 50.0)).sin()).round(),
                _ => {
                    f64::from(u8::from(i.is_multiple_of(97)))
                        * ((next() - 0.5) * 2.0 * f64::from(TOP)).floor()
                }
            };
            v.clamp(f64::from(BOTTOM), f64::from(TOP)) as i32
        };
        let channels = if stereo {
            let left = frames(n, &mut make);
            let right = frames(n, &mut make);
            vec![left, right]
        } else {
            vec![frames(n, &mut make)]
        };
        expect_same(&block(channels, round_));
    }
}

#[test]
fn codes_music_like_sound_in_well_under_the_raw_size() {
    let left = frames(FRAMES, |i| {
        round(2_000_000.0 * (i as f64 / 9.0).sin() + 300_000.0 * (i as f64 / 2.1).sin())
    });
    let right = left
        .iter()
        .enumerate()
        .map(|(i, v)| v + round(20_000.0 * (i as f64 / 4.0).sin()))
        .collect();
    let raw = (FRAMES * 2 * 3) as f64;
    assert!((pack(&block(vec![left, right], 7)).unwrap().len() as f64) < raw * 0.6);
}

#[test]
fn drops_the_eight_empty_bits_of_a_16_bit_source_the_commonest_case_there_is() {
    let mut next = random(6);
    let mut noise16 = || ((next() * 65_536.0).floor() as i32 - 32_768) * 256;
    let left = frames(FRAMES, |_| noise16());
    let right = frames(FRAMES, |_| noise16());
    let sixteen = block(vec![left, right], 7);
    let raw16 = (FRAMES * 2 * 2) as f64;

    expect_same(&sixteen);
    // 16-bit noise coded as 16-bit, not as 24.
    assert!((pack(&sixteen).unwrap().len() as f64) < raw16 * 1.03);
}

#[test]
fn keeps_noise_which_cannot_be_predicted_within_a_percent_and_a_half_of_its_raw_size() {
    // 0.6% over with the verbatim escape; 2.4% over in Rice codes alone.
    let mut next = random(5);
    let left = frames(FRAMES, |_| full_scale(&mut next));
    let right = frames(FRAMES, |_| full_scale(&mut next));
    let raw = (FRAMES * 2 * 3) as f64;
    assert!((pack(&block(vec![left, right], 7)).unwrap().len() as f64) < raw * 1.015);
}

#[test]
fn refuses_a_sample_that_is_not_24_bit_rather_than_wrapping_it() {
    assert!(matches!(
        pack(&block(vec![vec![TOP + 1]], 7)),
        Err(ProtocolError::Invalid { .. })
    ));
    assert!(matches!(
        pack(&block(vec![vec![BOTTOM - 1]], 7)),
        Err(ProtocolError::Invalid { .. })
    ));
}

#[test]
fn refuses_blocks_it_cannot_describe() {
    assert!(pack(&block(vec![], 7)).is_err());
    assert!(pack(&block(vec![vec![1], vec![2], vec![3]], 7)).is_err());
    assert!(pack(&block(vec![vec![]], 7)).is_err());
    assert!(pack(&block(vec![vec![1, 2], vec![3]], 7)).is_err());
    assert!(pack(&block(vec![vec![0; 0x1_0000]], 7)).is_err());
    assert!(pack(&block(vec![vec![0; 0xffff]], 7)).is_ok());
}

#[test]
fn refuses_a_packet_cut_short_with_a_reason_rather_than_garbage() {
    let bytes = pack(&block(
        vec![
            frames(FRAMES, |i| i as i32 * 1000),
            frames(FRAMES, |i| -(i as i32) * 1000),
        ],
        7,
    ))
    .unwrap();
    assert!(matches!(
        unpack(&bytes[..bytes.len() - 40]),
        Err(ProtocolError::Unexpected { .. })
    ));
    assert!(matches!(
        unpack(&bytes[..10]),
        Err(ProtocolError::Unexpected { .. })
    ));
}

#[test]
fn refuses_a_packet_from_another_version() {
    let mut bytes = pack(&block(vec![vec![1, 2, 3]], 7)).unwrap();
    bytes[0] = 9;
    assert!(matches!(
        unpack(&bytes),
        Err(ProtocolError::Unexpected { .. })
    ));
}

#[test]
fn a_float_is_exact_for_every_sample_a_16_or_24_bit_source_can_produce() {
    for v in [0, 1, -1, 12_345, -8_388_608, 8_388_607, 256, -256] {
        assert_eq!(to_int24(from_int24(v)), v);
    }
    for v in (-32_768..32_768).step_by(97) {
        assert_eq!(to_int24((f64::from(v) / 32_768.0) as f32), v * 256);
    }
}

#[test]
fn a_float_between_two_samples_rounds_to_the_nearer_on_either_side_of_zero() {
    let step = 1.0 / 8_388_608.0;
    assert_eq!(to_int24((1000.6 * step) as f32), 1001);
    assert_eq!(to_int24((1000.4 * step) as f32), 1000);
    assert_eq!(to_int24((-1000.6 * step) as f32), -1001);
    assert_eq!(to_int24((-1000.4 * step) as f32), -1000);
}

#[test]
fn a_float_exactly_between_two_samples_rounds_up_as_javascript_does() {
    // Rust's round goes away from zero, and would send -1001 here.
    let step = 1.0 / 8_388_608.0;
    assert_eq!(to_int24((1000.5 * step) as f32), 1001);
    assert_eq!(to_int24((-1000.5 * step) as f32), -1000);
}

#[test]
fn a_float_past_full_scale_clips_rather_than_wraps() {
    assert_eq!(to_int24(1.5), TOP);
    assert_eq!(to_int24(-1.5), BOTTOM);
    assert_eq!(to_int24(1.0), TOP);
    assert_eq!(to_int24(f32::INFINITY), TOP);
    assert_eq!(to_int24(f32::NAN), 0);
}

/// What Chrome does: negative over 2^(n-1), positive over 2^(n-1) - 1, to a 32-bit float.
fn chrome16(v: i32) -> f32 {
    if v < 0 {
        (f64::from(v) / 32_768.0) as f32
    } else {
        (f64::from(v) / 32_767.0) as f32
    }
}

fn chrome24(v: i32) -> f32 {
    let wide = f64::from(v) * 256.0;
    if wide < 0.0 {
        (wide / 2_147_483_648.0) as f32
    } else {
        (wide / 2_147_483_647.0) as f32
    }
}

#[test]
fn a_sample_chrome_decoded_is_every_16_bit_sample_exactly_shifted_into_24_bits() {
    let recover = decoded_to_int24(16);
    for v in -32_768..=32_767 {
        assert_eq!(recover(chrome16(v)), v * 256, "16-bit {v}");
    }
}

#[test]
fn a_sample_chrome_decoded_is_every_24_bit_sample_exactly_across_the_range_and_at_both_rails() {
    let recover = decoded_to_int24(24);
    let mut next = random(7);
    let mut values = vec![
        -8_388_608, -8_388_607, -1, 0, 1, 4_194_303, 4_194_304, 8_388_606, 8_388_607,
    ];
    for _ in 0..200_000 {
        values.push((next() * 16_777_216.0).floor() as i32 - 8_388_608);
    }
    for v in values {
        assert_eq!(recover(chrome24(v)), v, "24-bit {v}");
    }
}

#[test]
fn the_16_bit_recovery_is_not_what_the_one_scale_conversion_gives_which_is_why_it_exists() {
    assert_ne!(to_int24(chrome16(6827)), 6827 * 256);
}

#[test]
fn leaves_24_bit_files_and_anything_never_integers_to_the_one_scale_conversion() {
    // The web test compares the functions themselves; Rust function pointers
    // cannot be compared reliably, so the behaviour is, across the range.
    let mut next = random(8);
    for bits in [24, 0, 32] {
        let conversion = decoded_to_int24(bits);
        for _ in 0..10_000 {
            let sample = ((next() - 0.5) * 2.2) as f32;
            assert_eq!(
                conversion(sample),
                to_int24(sample),
                "{bits} bits, {sample}"
            );
        }
        assert_eq!(conversion(1.5), TOP);
    }
}
