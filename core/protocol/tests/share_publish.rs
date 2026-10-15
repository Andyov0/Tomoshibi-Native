//! What these guard is a setting that looks fine and fails slowly, and a
//! number that differs from the browser's without anything saying so.
//!
//! The cases are web/src/live/share.test.ts, sound.test.ts and delay.test.ts,
//! where they are about numbers rather than a browser. A screen share asked for
//! more than the machine can do does not fail loudly: the encoder falls behind
//! and the picture drifts. A share published as one layer looks completely
//! correct from the sending end, and the person on a thin path simply gets
//! nothing. A cushion on a camera puts the conversation itself behind. None of
//! it shows at the end that sends it, which is why it is held here.
//!
//! A few cases are this client's own, and say so: they hold the places where
//! the Rust SDK would turn the web's options into different encodings.

use tomoshibi_protocol::share::*;

const NAMED: [ShareQuality; 3] = [ShareQuality::P1080, ShareQuality::P1440, ShareQuality::P4k];
const M: u64 = 1_000_000;

fn capture(width: u32, height: u32) -> ShareCapture {
    ShareCapture { width: Some(width), height: Some(height), frame_rate: None }
}

fn at_rate(width: u32, height: u32, frame_rate: f64) -> ShareCapture {
    ShareCapture { width: Some(width), height: Some(height), frame_rate: Some(frame_rate) }
}

fn small() -> PublishLayer {
    share_rung_small()
}

fn middle() -> PublishLayer {
    share_rung_middle()
}

fn middle_fast() -> PublishLayer {
    share_rung_middle_fast()
}

/* -------------------------------------------------------------------------- */
/* What is offered                                                            */
/* -------------------------------------------------------------------------- */

#[test]
fn offers_every_rate_a_size_can_carry_slow_ones_included() {
    // A page of code at 4K wants fifteen frames and the sharpest picture, and
    // refusing that refuses the better answer.
    for quality in NAMED {
        assert!(share_rates_for(quality).contains(&15));
        assert!(share_rates_for(quality).contains(&30));
    }
}

#[test]
fn stops_each_size_where_an_encoder_would_stop_keeping_up() {
    assert_eq!(share_rates_for(ShareQuality::P1080), vec![15, 30, 60, 120, 240]);
    assert_eq!(share_rates_for(ShareQuality::P1440), vec![15, 30, 60, 120]);
    assert_eq!(share_rates_for(ShareQuality::P4k), vec![15, 30, 60]);
    assert_eq!(share_rates_for(ShareQuality::Auto), SHARE_FRAME_RATES.to_vec());

    assert!(!share_offers(ShareQuality::P4k, 120));
    assert!(!share_offers(ShareQuality::P1440, 240));
}

#[test]
fn clamps_a_rate_carried_over_from_a_size_that_allowed_it() {
    // The one that would be silent: 240 chosen at 1080p and remembered, then
    // 4K picked, must not reach an encoder that cannot do it.
    assert_eq!(share_profile(240, ShareQuality::P4k).frame_rate, 60);
    assert_eq!(share_profile(240, ShareQuality::P1440).frame_rate, 120);
    assert_eq!(share_profile(240, ShareQuality::P1080).frame_rate, 240);
}

/* -------------------------------------------------------------------------- */
/* What is sent                                                               */
/* -------------------------------------------------------------------------- */

#[test]
fn has_a_size_a_rate_and_a_bitrate_for_every_pairing_offered() {
    for quality in NAMED {
        for rate in share_rates_for(quality) {
            let got = share_profile(rate, quality);
            assert!(got.width > 0 && got.height > 0 && got.max_bitrate > 0);
            assert_eq!(got.frame_rate, rate);
        }
    }
}

#[test]
fn raises_the_bitrate_with_both_the_pixels_and_the_frames() {
    // More pixels or more frames at the same ceiling is the same bitrate spread
    // thinner, which is how a 4K option comes to look worse than 1080p.
    assert!(share_profile(30, ShareQuality::P4k).max_bitrate > share_profile(30, ShareQuality::P1080).max_bitrate);
    assert!(share_profile(120, ShareQuality::P1080).max_bitrate > share_profile(30, ShareQuality::P1080).max_bitrate);
}

#[test]
fn prices_a_picture_by_the_webs_own_formula() {
    // The web's figures as its comments quote them: 1440p at 120 is 48.7 Mb/s,
    // 1080p at fifteen 9.68, 1440p at sixty 34.4, and 1080p at 240 and 4K at
    // sixty under the 80 Mb/s cap. A rounding that differed would be a ceiling
    // that differs from a browser's by a bit, which nobody would ever notice and
    // the hint's comparison against the encoder's ceiling would.
    assert_eq!(share_bitrate_for(2560, 1440, 120.0), 48_660_480);
    assert_eq!(share_bitrate_for(1920, 1080, 15.0), 9_677_294);
    assert_eq!(share_profile(60, ShareQuality::P1440).max_bitrate, 34_408_155);
    assert_eq!(share_profile(60, ShareQuality::Auto).max_bitrate, 34_408_155);
    assert!(share_profile(240, ShareQuality::P1080).max_bitrate <= 80 * M);
    assert!(share_profile(60, ShareQuality::P4k).max_bitrate <= 80 * M);
}

#[test]
fn sends_h264_alone_and_never_a_second_codec() {
    // Never software, at any size or rate: a share is the one track everybody is
    // looking at, so one person's sharpness would be everybody's processor.
    assert_eq!(VIDEO_CODEC, "h264");
    const { assert!(!BACKUP_CODEC) };
}

#[test]
fn never_gives_away_the_picture_at_any_setting() {
    // A late frame is late; type that has been scaled away is gone.
    assert_eq!(share_profile(30, ShareQuality::Auto).degradation, VideoDegradation::MaintainResolution);
    for quality in NAMED {
        for rate in share_rates_for(quality) {
            assert_eq!(share_profile(rate, quality).degradation, VideoDegradation::MaintainResolution);
        }
    }
}

#[test]
fn says_what_the_pixels_are() {
    assert_eq!(share_profile(240, ShareQuality::Auto).content_hint, ShareContentHint::Detail);
    assert_eq!(share_profile(30, ShareQuality::P4k).content_hint, ShareContentHint::Text);
    assert_eq!(share_profile(15, ShareQuality::P1080).content_hint, ShareContentHint::Text);
    assert_eq!(share_profile(60, ShareQuality::P1440).content_hint, ShareContentHint::Motion);
}

#[test]
fn charges_far_less_for_frames_than_for_pixels() {
    let still = share_profile(30, ShareQuality::P1440).max_bitrate as f64;
    let moving = share_profile(120, ShareQuality::P1440).max_bitrate as f64;

    // Four times the frames, nothing like four times the bitrate.
    assert!(moving > still);
    assert!(moving < still * 2.5);

    // And a bigger picture still costs more than a faster one: 4K at sixty
    // once asked for less than 1080p at 240.
    assert!(share_profile(60, ShareQuality::P4k).max_bitrate > share_profile(240, ShareQuality::P1080).max_bitrate);
}

/* -------------------------------------------------------------------------- */
/* The small copies that go with a share                                      */
/* -------------------------------------------------------------------------- */

#[test]
fn goes_beneath_the_share_rather_than_instead_of_it() {
    let publish = share_publish(120, ShareQuality::P1080, capture(2560, 1440));

    assert!(publish.simulcast);
    assert_eq!(publish.rungs, vec![small(), middle_fast()]);

    // The half that matters most: nothing was taken off the top to pay for
    // them.
    assert_eq!(publish.encoding.max_bitrate, share_profile(120, ShareQuality::P1080).max_bitrate);
    assert_eq!(publish.encoding.max_framerate, 120.0);
}

#[test]
fn is_the_ladder_the_owner_chose() {
    // A decision, not a tuning: changing one is a conversation with the person
    // who made it, and this is what makes it one.
    let said = |rung: PublishLayer| (rung.width, rung.height, rung.max_bitrate, rung.max_framerate);

    assert_eq!(said(small()), (1280, 720, 2_500_000, 30.0));
    assert_eq!(said(middle()), (1920, 1080, 8_000_000, 30.0));
    assert_eq!(said(middle_fast()), (1920, 1080, 11_313_708, 60.0));
}

#[test]
fn prices_the_middle_rungs_sixty_by_the_top_layers_own_curve() {
    let curve = share_profile(60, ShareQuality::P1080).max_bitrate as f64
        / share_profile(30, ShareQuality::P1080).max_bitrate as f64;
    let rung = middle_fast().max_bitrate as f64 / middle().max_bitrate as f64;

    assert!((rung - curve).abs() < 1e-6, "{rung} against {curve}");
}

#[test]
fn sends_the_middle_rung_at_sixty_for_a_share_sent_at_sixty_or_more_and_at_thirty_otherwise() {
    for (rate, quality, middle) in [
        (15, ShareQuality::P1440, middle()),
        (30, ShareQuality::P1440, middle()),
        (60, ShareQuality::P1440, middle_fast()),
        (120, ShareQuality::P1440, middle_fast()),
        (60, ShareQuality::P4k, middle_fast()),
        // Automatic sends sixty, whatever rate it was handed.
        (30, ShareQuality::Auto, middle_fast()),
    ] {
        let rungs = share_rungs_beneath(capture(3840, 2160), share_profile(rate, quality).frame_rate as f64);
        assert_eq!(rungs, vec![small(), middle], "{quality:?} at {rate}");
    }
}

#[test]
fn goes_by_the_frames_the_capture_gives_where_it_gives_fewer_than_were_asked_for() {
    // Sixty chosen on a source that answers thirty: eleven megabits of ceiling
    // for thirty frames is upload reserved for nothing. Never the other way.
    let slow = share_publish(60, ShareQuality::P1440, at_rate(2560, 1440, 30.0));
    assert_eq!(slow.rungs.iter().map(|rung| rung.max_bitrate).collect::<Vec<_>>(), vec![2_500_000, 8_000_000]);

    let eager = share_publish(30, ShareQuality::P1440, at_rate(2560, 1440, 120.0));
    assert_eq!(eager.rungs, vec![small(), middle()]);
}

#[test]
fn keeps_the_bottom_rung_a_fraction_of_any_picture_it_stands_in_for() {
    for quality in SHARE_QUALITIES {
        for rate in share_rates_for(quality) {
            assert!(small().max_bitrate * 3 < share_profile(rate, quality).max_bitrate);
        }
    }
}

#[test]
fn climbs_so_a_small_window_is_not_sent_the_starved_picture() {
    for middle in [middle(), middle_fast()] {
        assert!(middle.height > small().height);
        assert!(middle.max_bitrate > small().max_bitrate);
        assert!(middle.max_framerate >= small().max_framerate);
    }

    // Smallest first by bitrate, which is the order the SDK sorts presets into
    // and the order a retune names each encoding's rung by.
    for rate in [15.0, 30.0, 60.0, 120.0, 240.0] {
        let ladder = share_ladder_at(rate);
        assert!(ladder.windows(2).all(|pair| pair[0].max_bitrate <= pair[1].max_bitrate));
    }
}

#[test]
fn publishes_a_capture_only_with_the_rungs_smaller_than_it() {
    // A rung as large as the capture is the original again at a lower
    // ceiling, which dynacast turns on whenever anybody watches at full size.
    for (width, height, rungs) in [
        (3840, 2160, vec![small(), middle()]),
        (2560, 1440, vec![small(), middle()]),
        (1920, 1200, vec![small(), middle()]),
        (1920, 1080, vec![small()]),
        (1366, 768, vec![small()]),
        // Taller than wide: the shorter side is what the SDK scales by.
        (1080, 1920, vec![small()]),
        (1280, 720, vec![]),
        (800, 600, vec![]),
    ] {
        assert_eq!(share_publish(30, ShareQuality::P4k, capture(width, height)).rungs, rungs, "{width}x{height}");

        // The same sizes at sixty, with the middle rung at sixty.
        let sixty: Vec<PublishLayer> =
            rungs.iter().map(|rung| if *rung == middle() { middle_fast() } else { *rung }).collect();
        assert_eq!(
            share_publish(60, ShareQuality::P4k, capture(width, height)).rungs,
            sixty,
            "{width}x{height} at sixty"
        );
    }
}

#[test]
fn says_none_rather_than_nothing_for_a_capture_too_small_for_any_rung() {
    assert_eq!(share_rungs_beneath(capture(1280, 720), 60.0), vec![]);

    // This client's own: the Rust SDK panics on an empty list for a capture
    // this wide, so the same single layer is asked for with simulcast off.
    let publish = share_publish(60, ShareQuality::Auto, capture(1280, 720));
    assert!(publish.rungs.is_empty());
    assert!(!publish.simulcast);
    assert!(share_publish(60, ShareQuality::Auto, capture(1920, 1080)).simulcast);
}

#[test]
fn keeps_every_rung_when_the_capture_will_not_say_how_large_it_is() {
    assert_eq!(share_rungs_beneath(ShareCapture::default(), 30.0), vec![small(), middle()]);
    assert_eq!(share_rungs_beneath(ShareCapture::default(), 60.0), vec![small(), middle_fast()]);
}

#[test]
fn holds_every_rung_to_the_shares_own_rate_as_the_browser_encodes_it() {
    // This client's own. The browser SDK takes the smaller of a rung's rate and
    // the top layer's as it builds the encodings; the Rust SDK copies the
    // preset's. Handed the presets as they are, a share at fifteen would send
    // its rungs at thirty under a top at fifteen.
    let publish = share_publish(15, ShareQuality::P1440, capture(2560, 1440));
    assert_eq!(publish.encoding.max_framerate, 15.0);
    assert!(publish.rungs.iter().all(|rung| rung.max_framerate == 15.0), "{:?}", publish.rungs);

    let fast = share_publish(120, ShareQuality::P1440, capture(2560, 1440));
    assert_eq!(fast.rungs.iter().map(|rung| rung.max_framerate).collect::<Vec<_>>(), vec![30.0, 60.0]);
}

/* -------------------------------------------------------------------------- */
/* What the capture is asked for                                              */
/* -------------------------------------------------------------------------- */

#[test]
fn carries_the_chosen_rate_through_to_the_request() {
    assert_eq!(share_capture_request(120, ShareQuality::P1080).frame_rate, 120);
    assert_eq!(share_capture_request(240, ShareQuality::P1080).frame_rate, 240);
    assert_eq!(share_capture_request(60, ShareQuality::Auto).frame_rate, 60);
}

#[test]
fn asks_for_the_clamped_rate_not_the_one_that_was_remembered() {
    assert_eq!(share_capture_request(240, ShareQuality::P4k).frame_rate, 60);
}

#[test]
fn asks_for_the_size_that_was_chosen_whatever_screen_the_window_is_on() {
    let asked = share_capture_request(60, ShareQuality::P4k);
    assert_eq!((asked.width, asked.height), (3840, 2160));
}

/* -------------------------------------------------------------------------- */
/* The ceiling follows the picture, not the request                           */
/* -------------------------------------------------------------------------- */

#[test]
fn comes_down_to_what_a_smaller_capture_is_worth() {
    // 4K was chosen; a 1080p window came back.
    let settled = share_settle(30, ShareQuality::P4k, capture(1920, 1080));
    assert_eq!(settled, Some(share_profile(30, ShareQuality::P1080).max_bitrate));
    assert!(settled.unwrap() < share_profile(30, ShareQuality::P4k).max_bitrate);

    // And the publish carries the setting's own ceiling with the correction
    // beside it, as the browser publishes and then settles.
    let publish = share_publish(30, ShareQuality::P4k, capture(1920, 1080));
    assert_eq!(publish.encoding.max_bitrate, share_profile(30, ShareQuality::P4k).max_bitrate);
    assert_eq!(publish.settled_bitrate, settled);
}

#[test]
fn does_not_move_when_the_capture_is_the_size_that_was_asked_for() {
    assert_eq!(share_settle(30, ShareQuality::P1080, capture(1920, 1080)), None);
}

#[test]
fn does_not_move_when_the_capture_will_not_say() {
    assert_eq!(share_settle(30, ShareQuality::P4k, ShareCapture::default()), None);
}

#[test]
fn comes_down_when_the_display_gave_fewer_frames_than_were_asked_for() {
    // 240 chosen, a sixty-hertz display: the size matches exactly, which is
    // the common case and the one the size check alone walked straight past.
    let settled = share_settle(240, ShareQuality::P1080, at_rate(1920, 1080, 60.0));
    assert_eq!(settled, Some(share_profile(60, ShareQuality::P1080).max_bitrate));
}

#[test]
fn never_rises_above_what_was_chosen() {
    assert_eq!(share_settle(30, ShareQuality::P1080, at_rate(1920, 1080, 240.0)), None);
}

#[test]
fn hands_back_the_whole_difference_for_a_small_window_with_no_floor_under_it() {
    let settled = share_settle(15, ShareQuality::P1080, at_rate(800, 600, 15.0)).unwrap();
    assert!(settled < 3 * M, "{settled}");
}

#[test]
fn is_not_thrown_away_by_a_change_of_setting_mid_share() {
    // retune once wrote the setting's own figure back over the correction, and
    // settle runs once at publish: one nudge of the rate restored the
    // over-allocation for good.
    let after_share = share_settle(15, ShareQuality::P1080, at_rate(800, 600, 15.0)).unwrap();
    let retuned = share_retune_encodings(30, ShareQuality::P1080, at_rate(800, 600, 15.0), 2);
    let top = retuned.encodings.last().unwrap();

    assert!(top.top);
    assert!(top.max_bitrate < 8 * M);
    assert!(top.max_bitrate <= after_share.max(3 * M));
}

/* -------------------------------------------------------------------------- */
/* Changing a share as it runs                                                */
/* -------------------------------------------------------------------------- */

#[test]
fn narrows_the_capture_to_the_new_setting() {
    let asked = share_retune_capture(240, ShareQuality::P4k);
    assert_eq!((asked.max_width, asked.max_height, asked.ideal_frame_rate), (3840, 2160, 60));
    assert_eq!(asked.content_hint, ShareContentHint::Motion);
}

#[test]
fn follows_the_middle_rung_down_to_thirty_and_back_up_to_sixty() {
    // The capture takes the rate it is asked for, as a display source does.
    let down = share_retune_encodings(30, ShareQuality::P1440, at_rate(2560, 1440, 30.0), 3);
    assert_eq!((down.encodings[1].max_bitrate, down.encodings[1].max_framerate), (8_000_000, 30.0));

    let up = share_retune_encodings(60, ShareQuality::P1440, at_rate(2560, 1440, 60.0), 3);
    assert_eq!((up.encodings[1].max_bitrate, up.encodings[1].max_framerate), (11_313_708, 60.0));
}

#[test]
fn never_gives_a_rung_more_frames_than_the_share() {
    let retuned = share_retune_encodings(15, ShareQuality::P1440, at_rate(2560, 1440, 15.0), 3);

    assert_eq!(retuned.encodings[0].max_framerate, 15.0);
    assert_eq!(retuned.encodings[1].max_framerate, 15.0);
    assert_eq!(retuned.encodings[1].max_bitrate, 8_000_000);
}

#[test]
fn gives_the_top_layer_the_setting_and_keeps_the_rungs_as_rungs() {
    // The fallback handed the top layer's ceiling stops being a fallback.
    let retuned = share_retune_encodings(120, ShareQuality::P1440, at_rate(2560, 1440, 120.0), 3);
    let rates: Vec<u64> = retuned.encodings.iter().map(|one| one.max_bitrate).collect();

    assert_eq!(rates, vec![2_500_000, 11_313_708, share_profile(120, ShareQuality::P1440).max_bitrate]);
    assert_eq!(retuned.encodings.iter().map(|one| one.rid.as_str()).collect::<Vec<_>>(), vec!["q", "h", "f"]);
    assert_eq!(retuned.degradation, VideoDegradation::MaintainResolution);
}

#[test]
fn rescales_every_rung_against_the_capture_as_it_is_now() {
    // The SDK works the scale out once, at publish; a share resized afterwards
    // kept a ratio taken against a picture that no longer exists.
    let retuned = share_retune_encodings(30, ShareQuality::P4k, at_rate(3840, 2160, 30.0), 3);
    let scales: Vec<Option<f64>> = retuned.encodings.iter().map(|one| one.scale_resolution_down_by).collect();

    assert_eq!(scales, vec![Some(3.0), Some(2.0), None]);

    // Never scaled up: a capture smaller than a rung leaves the rung at one.
    let narrowed = share_retune_encodings(30, ShareQuality::P1080, at_rate(1280, 720, 30.0), 3);
    assert_eq!(narrowed.encodings[1].scale_resolution_down_by, Some(1.0));
}

#[test]
fn writes_nothing_where_there_are_no_encodings() {
    assert!(share_retune_encodings(30, ShareQuality::P1080, capture(1920, 1080), 0).encodings.is_empty());
}

/* -------------------------------------------------------------------------- */
/* What a share is recorded as running at                                     */
/* -------------------------------------------------------------------------- */

#[test]
fn records_automatic_with_the_rate_it_was_handed_and_a_size_with_its_clamped_rate() {
    assert_eq!(share_recorded(ShareQuality::Auto, 30), ShareSetting { quality: ShareQuality::Auto, frame_rate: 30 });
    assert_eq!(share_recorded(ShareQuality::P4k, 240), ShareSetting { quality: ShareQuality::P4k, frame_rate: 60 });
}

#[test]
fn a_change_is_the_same_share_and_a_start_is_a_new_one() {
    let run = ShareRun::new();
    let first = run.begin(ShareQuality::P1440, 120, 1_000.0);
    let changed = run.change(ShareQuality::P1440, 30, 9_000.0);

    assert_eq!(changed.id, first.id);
    assert_eq!(changed.started, 1_000.0);
    assert_eq!(changed.since, 9_000.0);
    assert_eq!((changed.quality, changed.frame_rate), (ShareQuality::P1440, 30));

    // A share stopped from outside never passes through `end`, and the next
    // one must not inherit its start and skip its quiet time.
    let next = run.begin(ShareQuality::P1440, 120, 20_000.0);
    assert_ne!(next.id, first.id);
    assert_eq!(next.started, 20_000.0);

    run.end();
    assert_eq!(run.current(), None);
}

/* -------------------------------------------------------------------------- */
/* What was written down                                                      */
/* -------------------------------------------------------------------------- */

#[test]
fn remembers_the_quality_under_the_webs_names_and_otherwise_automatic() {
    for quality in SHARE_QUALITIES {
        assert_eq!(share_remembered_quality(Some(share_quality_key(quality))), quality);
    }
    assert_eq!(share_quality_key(ShareQuality::P4k), "4k");
    assert_eq!(share_remembered_quality(Some("8k".into())), ShareQuality::Auto);
    assert_eq!(share_remembered_quality(None), ShareQuality::Auto);
}

#[test]
fn remembers_the_frame_rate_and_otherwise_sixty() {
    assert_eq!(share_remembered_frame_rate(Some("120".into())), 120);
    assert_eq!(share_remembered_frame_rate(Some("240".into())), 240);
    assert_eq!(share_remembered_frame_rate(Some("45".into())), 60);
    assert_eq!(share_remembered_frame_rate(Some("fast".into())), 60);
    assert_eq!(share_remembered_frame_rate(None), 60);
}

/* -------------------------------------------------------------------------- */
/* The camera                                                                 */
/* -------------------------------------------------------------------------- */

#[test]
fn captures_a_phone_at_360_lines_and_a_tablet_or_computer_at_720() {
    assert_eq!(camera_capture(CameraDevice::Phone), CameraCapture { width: 640, height: 360, frame_rate: 20 });
    assert_eq!(camera_capture(CameraDevice::Tablet), CameraCapture { width: 1280, height: 720, frame_rate: 30 });
    assert_eq!(camera_capture(CameraDevice::Computer), CameraCapture { width: 1280, height: 720, frame_rate: 30 });
}

fn layer(width: u32, height: u32, max_bitrate: u64, max_framerate: f64) -> PublishLayer {
    PublishLayer { width, height, max_bitrate, max_framerate }
}

#[test]
fn publishes_a_720p_camera_on_the_sdks_own_ladder() {
    // 1.7 Mb/s at the top, where the Rust SDK left to itself asks 3.
    let camera = camera_publish(1280, 720).unwrap();

    assert_eq!(camera.encoding, PublishEncoding { max_bitrate: 1_700_000, max_framerate: 30.0 });
    assert_eq!(camera.rungs, vec![layer(320, 180, 160_000, 20.0), layer(640, 360, 450_000, 20.0)]);
    assert!(camera.simulcast);
    assert_eq!(camera.degradation, VideoDegradation::MaintainFramerate);
}

#[test]
fn publishes_a_phones_camera_over_the_smaller_rung_alone() {
    // Two layers for a capture between 480 and 960 wide, and the one beneath
    // is the smaller of the SDK's two, as the browser chooses it; the Rust SDK
    // given both keeps the larger, a second 640x360 under a 640x360 top.
    let camera = camera_publish(640, 360).unwrap();

    assert_eq!(camera.encoding, PublishEncoding { max_bitrate: 450_000, max_framerate: 20.0 });
    assert_eq!(camera.rungs, vec![layer(320, 180, 160_000, 20.0)]);
}

#[test]
fn publishes_a_four_by_three_camera_on_the_four_by_three_tables() {
    let camera = camera_publish(640, 480).unwrap();

    assert_eq!(camera.encoding, PublishEncoding { max_bitrate: 500_000, max_framerate: 20.0 });
    assert_eq!(camera.rungs, vec![layer(240, 180, 125_000, 20.0)]);

    let large = camera_publish(1440, 1080).unwrap();
    assert_eq!(large.encoding.max_bitrate, 2_300_000);
    assert_eq!(large.rungs, vec![layer(240, 180, 125_000, 20.0), layer(480, 360, 330_000, 20.0)]);
}

#[test]
fn publishes_a_tiny_camera_as_one_layer() {
    let camera = camera_publish(320, 180).unwrap();

    assert_eq!(camera.encoding, PublishEncoding { max_bitrate: 160_000, max_framerate: 20.0 });
    assert!(camera.rungs.is_empty());
    assert!(!camera.simulcast);
}

#[test]
fn refuses_a_camera_with_no_size() {
    assert!(camera_publish(0, 720).is_err());
}

/* -------------------------------------------------------------------------- */
/* Sound                                                                      */
/* -------------------------------------------------------------------------- */

#[test]
fn processes_a_voice_by_default_without_voice_isolation() {
    let capture = sound_profile(SoundKind::Voice { original: false }).capture;
    assert!(capture.echo_cancellation && capture.noise_suppression && capture.auto_gain_control);
    assert!(!capture.voice_isolation);
    assert_eq!(capture.channels, 1);
}

#[test]
fn leaves_a_voice_alone_in_original_sound_except_for_echo_cancellation() {
    let capture = sound_profile(SoundKind::Voice { original: true }).capture;
    assert!(capture.echo_cancellation);
    assert!(!capture.noise_suppression && !capture.auto_gain_control && !capture.voice_isolation);
}

#[test]
fn sends_a_voice_continuously_with_redundancy_at_more_than_the_sdks_default() {
    let voice = sound_profile(SoundKind::Voice { original: false });
    assert_eq!((voice.max_bitrate, voice.dtx, voice.red, voice.stereo), (VOICE_BITRATE, false, true, false));
    assert_eq!((voice.source, voice.name.as_str()), (MediaSource::Microphone, ""));
    assert_eq!(sound_profile(SoundKind::Voice { original: true }).max_bitrate, ORIGINAL_BITRATE);
    const { assert!(VOICE_BITRATE > 48_000) };
}

#[test]
fn sends_a_shares_sound_as_music_rather_than_as_a_voice() {
    // Left to the defaults, a shared video's soundtrack was folded to mono,
    // gated by DTX and run through noise suppression.
    let sound = sound_profile(SoundKind::ScreenSound);
    assert_eq!((sound.source, sound.name.as_str()), (MediaSource::ScreenShareAudio, ""));
    assert_eq!((sound.max_bitrate, sound.stereo, sound.dtx, sound.red), (MUSIC_BITRATE, true, false, true));

    let capture = sound.capture;
    assert!(!capture.echo_cancellation && !capture.noise_suppression && !capture.auto_gain_control);
    assert_eq!((capture.channels, capture.sample_rate), (2, Some(48_000)));
    assert!(capture.exclude_own_audio);
}

#[test]
fn publishes_sound_on_its_own_at_opuss_ceiling_without_the_red_that_would_sink_it() {
    let listening = sound_profile(SoundKind::Listening { lossless_offered: false });

    assert_eq!((listening.source, listening.name.as_str()), (MediaSource::ScreenShareAudio, "listening"));
    assert_eq!((listening.stereo, listening.dtx, listening.red), (true, false, false));
    assert_eq!(listening.max_bitrate, 510_000);
}

#[test]
fn offers_the_lossless_stream_under_its_own_name_and_never_in_an_encrypted_call() {
    let offered = sound_lossless_offered(true, false, true);
    assert_eq!(sound_profile(SoundKind::Listening { lossless_offered: offered }).name, "listening-lossless");

    assert!(!sound_lossless_offered(true, true, true));
    assert!(!sound_lossless_offered(true, false, false));
    assert!(!sound_lossless_offered(false, false, true));
}

#[test]
fn gives_the_song_desk_a_channel_of_its_own() {
    // Published as a share's sound, a song went when a screen share stopped.
    let music = sound_profile(SoundKind::LibraryMusic { lossless_offered: false });
    assert_eq!((music.source, music.name.as_str()), (MediaSource::Unknown, "music"));
    assert_eq!((music.max_bitrate, music.red), (510_000, false));
    assert_eq!(sound_profile(SoundKind::LibraryMusic { lossless_offered: true }).name, "music-lossless");
}

#[test]
fn does_not_double_sound_already_shared_on_its_own() {
    assert!(share_sound_alongside(false));
    assert!(!share_sound_alongside(true));
}

/* -------------------------------------------------------------------------- */
/* The cushion                                                                */
/* -------------------------------------------------------------------------- */

#[test]
fn holds_shares_a_fifth_of_a_second_when_nothing_has_been_kept() {
    assert_eq!(SHARE_PLAYOUT_DEFAULT_MS, 200);
    assert_eq!(share_playout_ms(None), 200);
}

#[test]
fn is_off_when_the_switch_says_nought_and_nothing_else_turns_it_off() {
    assert_eq!(share_playout_ms(Some("0".into())), 0);

    // A stray value is not somebody asking for it off: the default stands.
    for kept in ["-200", "1.5", "5000", "on", "", " "] {
        assert_eq!(share_playout_ms(Some(kept.into())), 200, "{kept:?}");
    }
}

#[test]
fn is_what_the_switch_says_in_whole_milliseconds_up_to_a_second() {
    for (kept, read) in [("1", 1), ("120", 120), ("1000", 1000)] {
        assert_eq!(share_playout_ms(Some(kept.into())), read, "{kept}");
    }
}

#[test]
fn holds_a_shares_picture_and_its_sound_together_and_nobodys_face_or_voice() {
    let ms = share_playout_ms(None);

    assert_eq!(share_cushion_ms(MediaSource::ScreenShare, ms), Some(200));
    assert_eq!(share_cushion_ms(MediaSource::ScreenShareAudio, ms), Some(200));
    assert_eq!(share_cushion_ms(MediaSource::Camera, ms), None);
    assert_eq!(share_cushion_ms(MediaSource::Microphone, ms), None);
    // The song desk's music is on a source of its own, which delay.ts does not
    // hold.
    assert_eq!(share_cushion_ms(MediaSource::Unknown, ms), None);
}

#[test]
fn touches_nothing_while_the_switch_is_off() {
    for source in [MediaSource::ScreenShare, MediaSource::ScreenShareAudio] {
        assert_eq!(share_cushion_ms(source, 0), None);
    }
}
