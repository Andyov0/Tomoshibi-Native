//! The start-bitrate hint the SDK writes into the offer that publishes the
//! first camera (vendor/livekit, peer_transport.rs).
//!
//! In single PC mode that offer also carries every remote video being
//! received, each in a recvonly m-section. A changed fmtp there is a change
//! of receive codecs to libwebrtc, which rebuilds the running receive
//! stream; a packet arriving in the gap took libwebrtc's unsignalled-SSRC
//! path, which set the picture's sink to nothing, and the remote camera
//! stayed black for the rest of the call (one launch in four on Windows,
//! joining three cameras and publishing one). Only the sections that send
//! may change.

use livekit::rtc_engine::munge_start_bitrate;

/// The shape of a real single PC offer: an audio section, two remote
/// cameras (one received, one not yet negotiated), and the camera being
/// published.
const OFFER: &str = "v=0\r\n\
o=- 0 0 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
a=mid:0\r\n\
a=recvonly\r\n\
a=rtpmap:111 opus/48000/2\r\n\
m=video 9 UDP/TLS/RTP/SAVPF 96 103\r\n\
a=mid:3\r\n\
a=recvonly\r\n\
a=rtpmap:96 VP8/90000\r\n\
a=rtpmap:103 H264/90000\r\n\
a=fmtp:103 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f\r\n\
m=video 9 UDP/TLS/RTP/SAVPF 96 103\r\n\
a=mid:4\r\n\
a=inactive\r\n\
a=rtpmap:96 VP8/90000\r\n\
a=rtpmap:103 H264/90000\r\n\
m=video 9 UDP/TLS/RTP/SAVPF 96 103\r\n\
a=mid:9\r\n\
a=sendonly\r\n\
a=rtpmap:96 VP8/90000\r\n\
a=rtpmap:103 H264/90000\r\n\
a=fmtp:103 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f\r\n";

fn sections(sdp: &str) -> Vec<&str> {
    sdp.split("m=").collect()
}

#[test]
fn the_sections_that_receive_are_left_exactly_as_they_were() {
    let out = munge_start_bitrate(OFFER, 1000);
    let (before, after) = (sections(OFFER), sections(&out));
    assert_eq!(after.len(), before.len());
    // The session part, the audio and both receiving cameras.
    for index in 0..4 {
        assert_eq!(after[index], before[index], "section {index} changed");
    }
}

#[test]
fn the_camera_being_sent_gets_the_hint_on_every_codec() {
    let out = munge_start_bitrate(OFFER, 1000);
    let camera = sections(&out)[4];
    assert!(camera.contains("a=rtpmap:96 VP8/90000\r\na=fmtp:96 x-google-start-bitrate=1000\r\n"), "{camera}");
    assert!(camera.contains("profile-level-id=42e01f;x-google-start-bitrate=1000\r\n"), "{camera}");
    assert_eq!(out.matches("x-google-start-bitrate").count(), 2);
    assert!(out.ends_with("\r\n") && !out.ends_with("\r\n\r\n"));
}

#[test]
fn a_video_section_with_no_direction_sends() {
    let sdp = "v=0\n\
m=video 9 UDP/TLS/RTP/SAVPF 96\n\
a=rtpmap:96 VP8/90000\n\
m=video 9 UDP/TLS/RTP/SAVPF 97\n\
a=recvonly\n\
a=rtpmap:97 VP8/90000\n";
    let out = munge_start_bitrate(sdp, 700);
    assert!(out.contains("a=rtpmap:96 VP8/90000\na=fmtp:96 x-google-start-bitrate=700\n"), "{out}");
    assert!(!out.contains("a=fmtp:97"), "{out}");
}
