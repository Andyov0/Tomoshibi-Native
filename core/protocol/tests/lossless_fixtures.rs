//! The lossless stream against the web client's own output.
//!
//! The fixtures were written by web/src/live/lossless.ts and lossless-sender.ts
//! themselves (tests/fixtures/lossless/generate.mjs says how to make them
//! again). Every packet here has to come out byte for byte, and every packet
//! the browser wrote, or that arrived damaged, has to decode to what the
//! browser decoded it to, error for error.

use std::collections::HashSet;

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use tomoshibi_protocol::lossless::{
    decoded_to_int24, pack, to_int24, unpack, Block, Channel, LosslessSender, Sound,
};
use tomoshibi_protocol::ProtocolError;

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("a fixture is JSON")
}

fn bytes(value: &Value) -> Vec<u8> {
    STANDARD
        .decode(value.as_str().expect("base64 is a string"))
        .expect("base64")
}

fn ints(value: &Value) -> Vec<i32> {
    bytes(value)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&c| i32::from_le_bytes(c))
        .collect()
}

fn floats(value: &Value) -> Vec<f32> {
    bytes(value)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&c| f32::from_le_bytes(c))
        .collect()
}

fn number(value: &Value) -> u64 {
    value.as_u64().expect("a whole number")
}

fn channels(value: &Value) -> Vec<Vec<i32>> {
    value
        .as_array()
        .expect("an array of channels")
        .iter()
        .map(ints)
        .collect()
}

#[test]
fn every_block_packs_to_the_web_clients_bytes_and_back() {
    let fixture = json(include_str!("fixtures/lossless/blocks.json"));
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() > 60, "the fixture has its cases");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let block = Block {
            rate: number(&case["rate"]) as u32,
            stream: number(&case["stream"]) as u32,
            seq: number(&case["seq"]) as u32,
            channels: channels(&case["samples"]),
        };
        let packet = bytes(&case["packet"]);

        let packed = pack(&block).unwrap_or_else(|e| panic!("{name}: {e}"));
        if packed != packet {
            let at = packed
                .iter()
                .zip(&packet)
                .position(|(a, b)| a != b)
                .unwrap_or(packed.len().min(packet.len()));
            panic!(
                "{name}: {} bytes where the web client wrote {}, first different at byte {at}",
                packed.len(),
                packet.len()
            );
        }

        assert_eq!(
            unpack(&packet).unwrap_or_else(|e| panic!("{name}: {e}")),
            block,
            "{name}"
        );

        // The 16-bit path starts from what Chrome decoded the file to.
        if let Some(chrome) = case.get("floats") {
            let recover = decoded_to_int24(16);
            let recovered: Vec<Vec<i32>> = chrome
                .as_array()
                .unwrap()
                .iter()
                .map(|plane| floats(plane).into_iter().map(recover).collect())
                .collect();
            assert_eq!(
                recovered, block.channels,
                "{name}: the 16-bit file's own integers"
            );
        }
    }
}

#[test]
fn a_damaged_packet_decodes_as_it_does_in_the_browser() {
    let fixture = json(include_str!("fixtures/lossless/malformed.json"));
    let cases = fixture["cases"].as_array().unwrap();
    let mut decoded = 0;
    let mut refused = 0;

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let packet = if let Some(base) = case.get("base") {
            let whole = bytes(&fixture["bases"][base.as_str().unwrap()]);
            whole[..number(&case["length"]) as usize].to_vec()
        } else if case["packet"].is_null() {
            // The runaway case, too big to keep: its note says what it was.
            assert_eq!(name, "runaway");
            let mut bytes = vec![0u8; 16 + 9_000_000];
            bytes[..3].copy_from_slice(&[1, 1, 1]);
            bytes
        } else {
            bytes(&case["packet"])
        };

        match (unpack(&packet), case["error"].as_str()) {
            (Err(ProtocolError::Unexpected { detail: message }), Some(expected)) => {
                assert_eq!(message, expected, "{name}");
                refused += 1;
            }
            (Ok(block), None) => {
                assert_eq!(u64::from(block.rate), number(&case["rate"]), "{name}");
                assert_eq!(u64::from(block.stream), number(&case["stream"]), "{name}");
                assert_eq!(u64::from(block.seq), number(&case["seq"]), "{name}");
                assert_eq!(block.channels, channels(&case["samples"]), "{name}");
                decoded += 1;
            }
            (got, expected) => panic!("{name}: got {got:?}, the web client {expected:?}"),
        }
    }

    // Both outcomes are well represented, or the fixture proves little.
    assert!(
        decoded > 100 && refused > 300,
        "{decoded} decoded, {refused} refused"
    );
}

#[test]
fn floats_become_the_samples_the_browser_stores() {
    let fixture = json(include_str!("fixtures/lossless/conversions.json"));
    let sixteen = decoded_to_int24(16);

    for row in fixture["values"].as_array().unwrap() {
        let float = f32::from_bits(number(&row[0]) as u32);
        let one_scale = row[1].as_i64().unwrap() as i32;
        let chrome = row[2].as_i64().unwrap() as i32;
        assert_eq!(
            to_int24(float),
            one_scale,
            "toInt24({float:e}), bits {:#010x}",
            float.to_bits()
        );
        assert_eq!(
            sixteen(float),
            chrome,
            "decodedToInt24(16)({float:e}), bits {:#010x}",
            float.to_bits()
        );
    }
}

#[test]
fn the_sender_publishes_what_the_web_clients_sender_published() {
    let fixture = json(include_str!("fixtures/lossless/sender.json"));

    for run in fixture["runs"].as_array().unwrap() {
        let name = run["name"].as_str().unwrap();
        let channel = Channel::of(if run["channel"] == "music" {
            Sound::Music
        } else {
            Sound::Screen
        });
        let to_int = if run["bits"].is_null() {
            to_int24
        } else {
            decoded_to_int24(number(&run["bits"]) as u32)
        };
        let mut streams = run["streams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| number(id) as u32)
            .collect::<Vec<_>>()
            .into_iter();
        let mut sender = LosslessSender::new(channel, to_int)
            .with_streams(move || streams.next().expect("a stream id"));

        // An upload that never finishes a send: the first packet taken is in
        // flight for good, and nothing more is taken.
        let stalled = run["stalled"].as_bool().unwrap();
        let mut in_flight = false;
        let mut present = HashSet::new();
        let mut gave_up = 0;
        let now = 1_700_000_000_000;

        for (index, step) in run["steps"].as_array().unwrap().iter().enumerate() {
            let mut captured = None;
            if let Some(who) = step.get("join") {
                present.insert(who.as_str().unwrap().to_owned());
            }
            if let Some(who) = step.get("vanish") {
                present.remove(who.as_str().unwrap());
            }
            if let Some(ask) = step.get("ask") {
                sender.on_ask(
                    now,
                    ask["from"].as_str().unwrap(),
                    ask["topic"].as_str().unwrap(),
                    &bytes(&ask["payload"]),
                );
            }
            if let Some(chunk) = step.get("chunk") {
                let planes: Vec<Vec<f32>> = chunk["planes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|plane| match plane.get("silence") {
                        Some(frames) => vec![0.0; number(frames) as usize],
                        None => floats(plane),
                    })
                    .collect();
                let timestamp = chunk["timestamp"].as_i64().unwrap();
                let rate = number(&chunk["rate"]) as u32;
                captured = Some(
                    sender
                        .capture(now, rate, timestamp, &planes, |identity| {
                            present.contains(identity)
                        })
                        .unwrap_or_else(|e| panic!("{name} step {index}: {e}")),
                );
            }

            let mut published = Vec::new();
            if stalled {
                if !in_flight {
                    if let Some(packet) = sender.next_packet() {
                        published.push(packet);
                        in_flight = true;
                    }
                }
            } else {
                while let Some(packet) = sender.next_packet() {
                    published.push(packet);
                }
            }

            let expected = step["published"].as_array().unwrap();
            assert_eq!(
                published.len(),
                expected.len(),
                "{name} step {index}: packets published"
            );
            for (packet, want) in published.iter().zip(expected) {
                assert_eq!(
                    packet.topic,
                    want["topic"].as_str().unwrap(),
                    "{name} step {index}"
                );
                let to: Vec<&str> = want["to"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|who| who.as_str().unwrap())
                    .collect();
                assert_eq!(packet.to, to, "{name} step {index}: destinations, in order");
                assert!(
                    packet.payload == bytes(&want["payload"]),
                    "{name} step {index}: the packet differs"
                );
            }

            let up = captured.is_some_and(|c| c.gave_up);
            assert_eq!(
                up,
                step["gaveUp"].as_bool().unwrap(),
                "{name} step {index}: giving up"
            );
            gave_up += usize::from(up);
        }

        assert_eq!(
            sender.gaps(),
            number(&run["gaps"]),
            "{name}: holes in the capture"
        );
        assert_eq!(
            gave_up as u64,
            number(&run["gaveUp"]),
            "{name}: times given up"
        );
    }
}
