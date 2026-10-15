//! Keeping a player to the show.
//!
//! The first group is the web client's own (web/src/live/watch.test.ts,
//! "keeping a player to the show"). The rest hold the loop in
//! web/src/components/room/WatchScreen.tsx, which has no tests of its own in
//! the web: what would go wrong quietly there is a player asked to jump four
//! times a second while the last jump is still landing, a YouTube player told
//! to run five per cent fast (it cannot, and silently stays behind), a live
//! stream dragged to a position that means nothing, and a refused YouTube
//! play never said.

use tomoshibi_protocol::watch::{
    watch_correction, watch_expected, WatchCorrection, WatchKeeper, WatchPlayerReading, WatchShow, WatchVideo,
};
use tomoshibi_protocol::watch::player::{
    CLOSE_ENOUGH, FAST_NUDGE, NUDGE, SEEK_AT, SEEK_GAP_MS, SETTLED, YOUTUBE_REFUSED_MS,
};

fn rate(rate: f64) -> WatchCorrection {
    WatchCorrection { seek: None, rate }
}

fn seek(to: f64) -> WatchCorrection {
    WatchCorrection { seek: Some(to), rate: 1.0 }
}

mod correction {
    use super::*;

    #[test]
    fn leaves_a_player_that_is_close_enough_alone() {
        assert_eq!(watch_correction(10.0 + CLOSE_ENOUGH / 2.0, 10.0, true, 1.0), rate(1.0));
    }

    #[test]
    fn runs_a_player_that_is_a_little_behind_a_little_fast_and_one_a_little_ahead_a_little_slow() {
        assert_eq!(watch_correction(9.7, 10.0, true, 1.0), rate(1.0 + NUDGE));
        assert_eq!(watch_correction(10.3, 10.0, true, 1.0), rate(1.0 - NUDGE));
    }

    #[test]
    fn runs_faster_to_close_a_gap_of_a_second_which_a_seek_over_the_internet_leaves() {
        assert_eq!(watch_correction(9.0, 10.0, true, 1.0), rate(1.0 + FAST_NUDGE));
        assert_eq!(watch_correction(11.0, 10.0, true, 1.0), rate(1.0 - FAST_NUDGE));
    }

    #[test]
    fn closes_a_gap_it_has_started_on_all_the_way_not_just_inside_the_line_it_started_at() {
        let between = 10.0 - (CLOSE_ENOUGH + SETTLED) / 2.0;
        assert_eq!(watch_correction(between, 10.0, true, 1.0), rate(1.0));
        assert_eq!(watch_correction(between, 10.0, true, 1.0 + NUDGE), rate(1.0 + NUDGE));
        assert_eq!(watch_correction(10.0 - SETTLED / 2.0, 10.0, true, 1.0 + NUDGE), rate(1.0));
    }

    #[test]
    fn jumps_a_player_that_is_far_out_either_way() {
        assert_eq!(watch_correction(10.0 - SEEK_AT - 1.0, 10.0, true, 1.0), seek(10.0));
        assert_eq!(watch_correction(10.0 + SEEK_AT + 1.0, 10.0, true, 1.0), seek(10.0));
    }

    #[test]
    fn moves_a_paused_player_to_where_the_show_paused_and_runs_nothing() {
        assert_eq!(watch_correction(10.05, 10.0, false, 1.0), rate(1.0));
        assert_eq!(watch_correction(10.25, 10.0, false, 1.0), seek(10.0));
    }

    #[test]
    fn reckons_where_the_show_is_from_what_was_heard_and_how_long_ago() {
        let show = WatchShow {
            holder: "h".into(),
            holder_name: String::new(),
            queue: Vec::new(),
            playing: true,
            position: 10.0,
            version: 1,
            now: Some(WatchVideo { key: "k".into(), by: "b".into(), ticket: "ticket-xxxxxxxxxxxxxxx".into(), duration: 30.0, live: false }),
        };
        assert_eq!(watch_expected(show.clone(), 1000, 6000), 15.0);
        assert_eq!(watch_expected(WatchShow { playing: false, ..show.clone() }, 1000, 6000), 10.0);
        assert_eq!(watch_expected(show, 1000, 100_000), 30.0);
    }
}

mod keeping {
    use super::*;

    fn element(time: f64, playing: bool, rate: f64) -> WatchPlayerReading {
        WatchPlayerReading { ready: true, time, playing, rate, nudges: true, youtube: false }
    }

    fn youtube(time: f64, playing: bool) -> WatchPlayerReading {
        WatchPlayerReading { ready: true, time, playing, rate: 1.0, nudges: false, youtube: true }
    }

    #[test]
    fn leaves_a_player_in_step_alone() {
        let keeper = WatchKeeper::new();
        let step = keeper.keep(element(10.05, true, 1.0), true, 10.0, false, 1000);
        assert!(!step.play && !step.pause && !step.waiting && !step.refused);
        assert_eq!((step.seek, step.rate, step.position), (None, Some(1.0), Some(10.05)));
    }

    #[test]
    fn starts_a_stopped_player_from_where_the_show_is_rather_than_chasing_it() {
        let keeper = WatchKeeper::new();
        let step = keeper.keep(element(5.0, false, 1.0), true, 10.0, false, 1000);
        assert!(step.play);
        assert_eq!(step.seek, Some(10.0));
        // Landed where it was sent, so nothing is left to nudge.
        assert_eq!((step.rate, step.position), (Some(1.0), Some(10.0)));
    }

    #[test]
    fn pauses_a_player_when_the_show_pauses_and_moves_it_to_where_the_show_stopped() {
        let keeper = WatchKeeper::new();
        let step = keeper.keep(element(10.3, true, 1.05), false, 10.0, false, 1000);
        assert!(step.pause && !step.play);
        assert_eq!((step.seek, step.rate), (Some(10.0), Some(1.0)));
    }

    #[test]
    fn jumps_at_most_once_while_a_jump_is_still_landing() {
        let keeper = WatchKeeper::new();
        // A player that goes on reading where it was until the jump lands.
        let far = element(0.0, true, 1.0);
        assert_eq!(keeper.keep(far, true, 30.0, false, 1000).seek, Some(30.0));
        assert_eq!(keeper.keep(far, true, 30.25, false, 1250).seek, None);
        assert_eq!(keeper.keep(far, true, 31.0, false, 1000 + SEEK_GAP_MS - 1).seek, None);
        assert_eq!(keeper.keep(far, true, 31.5, false, 1000 + SEEK_GAP_MS).seek, Some(31.5));
    }

    #[test]
    fn does_not_jump_a_player_back_that_the_viewer_has_just_moved() {
        let keeper = WatchKeeper::new();
        keeper.seeked(1000);
        assert_eq!(keeper.keep(element(60.0, true, 1.0), true, 30.0, false, 1250).seek, None);
        assert_eq!(keeper.keep(element(60.0, true, 1.0), true, 30.0, false, 1000 + SEEK_GAP_MS).seek, Some(30.0));
    }

    #[test]
    fn jumps_youtubes_player_past_a_second_since_it_cannot_run_a_little_fast() {
        let keeper = WatchKeeper::new();
        let step = keeper.keep(youtube(8.5, true), true, 10.0, false, 1000);
        assert_eq!((step.seek, step.rate), (Some(10.0), None));

        let keeper = WatchKeeper::new();
        let step = keeper.keep(youtube(9.5, true), true, 10.0, false, 1000);
        assert_eq!((step.seek, step.rate), (None, None));
    }

    #[test]
    fn says_youtubes_player_was_refused_once_it_has_not_begun_well_after_being_asked() {
        let keeper = WatchKeeper::new();
        let first = keeper.keep(youtube(10.0, false), true, 10.0, false, 1000);
        assert!(first.play && !first.refused);
        let soon = keeper.keep(youtube(10.0, false), true, 10.25, false, 1000 + YOUTUBE_REFUSED_MS);
        assert!(soon.play && !soon.refused);
        let later = keeper.keep(youtube(10.0, false), true, 10.5, false, 1001 + YOUTUBE_REFUSED_MS);
        assert!(later.refused);

        // Playing at last, and the wait starts over the next time.
        keeper.keep(youtube(10.6, true), true, 10.6, false, 3000);
        let again = keeper.keep(youtube(20.0, false), true, 20.0, false, 4000);
        assert!(again.play && !again.refused);
    }

    #[test]
    fn follows_only_playing_or_paused_for_something_live() {
        let keeper = WatchKeeper::new();
        let step = keeper.keep(element(3.0, false, 1.0), true, 500.0, true, 1000);
        assert!(step.play);
        assert_eq!((step.seek, step.rate, step.position), (None, None, Some(3.0)));

        let step = keeper.keep(element(3.0, true, 1.0), false, 500.0, true, 1250);
        assert!(step.pause);
        assert_eq!(step.seek, None);
    }

    #[test]
    fn waits_for_a_player_that_is_not_ready_showing_where_everybody_is() {
        let keeper = WatchKeeper::new();
        let not_ready = WatchPlayerReading { ready: false, ..element(0.0, false, 1.0) };
        let step = keeper.keep(not_ready, true, 42.0, false, 1000);
        assert!(step.waiting && !step.play);
        assert_eq!((step.seek, step.position), (None, Some(42.0)));
        assert_eq!(keeper.keep(not_ready, true, 42.0, true, 1000).position, None);
    }
}
