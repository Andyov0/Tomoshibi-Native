//! Watching together: the show, as run here and as heard from elsewhere.
//!
//! The web client's own cases (web/src/live/watch.test.ts), ported one for
//! one. What would go wrong quietly: a pause from somebody who is not running
//! the show undone by the show's next word; a late, older message putting
//! everybody back where they were; a link that failed for somebody else
//! reported to nobody; a show everybody still believes in after the person
//! running it has gone; and anything at all said in a sealed call.
//!
//! And, since a film from a media server is relayed only for whoever chose
//! it: a link read by the holder for somebody else without saying so, which is
//! the holder spending their grant on a guest's paste with nothing on the
//! screen to show it; a ticket from somebody else queued without the server
//! being asked what it plays; and a refusal that names a permission arriving
//! as "could not be read", or naming the holder's permission to somebody who
//! asked.
//!
//! The web's tests drive a `Theatre` over a fake room whose lookups answer at
//! once; here the room is a small harness that publishes nothing, keeps what
//! would have been published, and answers every ask the moment it is made,
//! which is what the web's `await settle()` waits for.

use std::sync::Arc;

use serde_json::{json, Value};
use tomoshibi_protocol::watch::{
    read_show, WatchAnswer, WatchAsk, WatchOutput, WatchShow, WatchTheatre, WatchVideo, OVERRUN_S, STALE_MS,
    STARTING_MS, TELL_EVERY, TELL_STILL, TOPIC,
};

fn ticket_of(title: &str) -> String {
    format!("{:x<22}", format!("ticket-{title}"))
}

type Answerer = Box<dyn Fn(&str) -> WatchAnswer>;

fn found(ticket: String, duration: f64) -> WatchAnswer {
    WatchAnswer::Found { ticket, duration, live: false, grantor: None }
}

struct Room {
    theatre: Arc<WatchTheatre>,
    clock: u64,
    /// Everything published: the message, and to whom (empty is everybody).
    sent: Vec<(Value, Vec<String>)>,
    /// The same, as the bytes went out: a parsed value sorts its keys.
    raw: Vec<String>,
    /// Links read, and whether for somebody else.
    resolved: Vec<(String, bool)>,
    /// Tickets read.
    read: Vec<String>,
    /// Refusals said here.
    refused: Vec<String>,
    resolve: Answerer,
    reading: Answerer,
}

impl Room {
    fn new(identity: &str, others: &[&str]) -> Self {
        Self::sealed(identity, others, false)
    }

    fn sealed(identity: &str, others: &[&str], sealed: bool) -> Self {
        let theatre = WatchTheatre::new(identity.into(), "Holder".into(), sealed);
        let mut room = Self {
            theatre,
            clock: 1000,
            sent: Vec::new(),
            raw: Vec::new(),
            resolved: Vec::new(),
            read: Vec::new(),
            refused: Vec::new(),
            // What a link plays: anything with "bad" in it is not a video.
            resolve: Box::new(|text: &str| {
                if text.contains("bad") {
                    WatchAnswer::Refused { why: "not_a_video".into() }
                } else {
                    found(ticket_of(text.rsplit('/').next().unwrap_or(text)), 100.0)
                }
            }),
            // What the server says of a ticket somebody else read: anything this
            // file minted reads, anything else is what an expired or refused one is.
            reading: Box::new(|ticket: &str| {
                if ticket.starts_with("ticket-") {
                    found(ticket.to_string(), 300.0)
                } else {
                    WatchAnswer::Refused { why: "failed".into() }
                }
            }),
        };
        let out = room.theatre.roster(others.iter().map(|one| one.to_string()).collect(), room.clock);
        room.take(out);
        room
    }

    fn take(&mut self, out: WatchOutput) {
        for packet in out.packets {
            assert_eq!(packet.topic, TOPIC);
            assert!(packet.reliable);
            self.sent.push((serde_json::from_slice(&packet.payload).unwrap(), packet.destinations));
            self.raw.push(String::from_utf8(packet.payload).unwrap());
        }
        self.refused.extend(out.refusals);
        for ask in out.asks {
            let (request, answer) = match ask {
                WatchAsk::Resolve { request, text, for_other } => {
                    let answer = (self.resolve)(&text);
                    self.resolved.push((text, for_other));
                    (request, answer)
                }
                WatchAsk::Read { request, ticket } => {
                    let answer = (self.reading)(&ticket);
                    self.read.push(ticket);
                    (request, answer)
                }
            };
            let out = self.theatre.answer(request, answer, self.clock);
            self.take(out);
        }
    }

    fn send(&mut self, from: &str, message: Value) {
        let out = self.theatre.on_message(from.into(), from.to_uppercase(), message.to_string().into_bytes(), self.clock);
        self.take(out);
    }

    fn show(&self) -> Option<WatchShow> {
        self.theatre.state(self.clock).show
    }

    fn now(&self) -> Option<WatchVideo> {
        self.show().and_then(|show| show.now)
    }

    fn position(&self) -> f64 {
        self.show().map_or(-1.0, |show| show.position)
    }

    fn playing_now(&self) -> bool {
        self.show().is_some_and(|show| show.playing)
    }

    fn key(&self) -> String {
        self.now().map(|video| video.key).unwrap_or_default()
    }

    fn last(&self) -> (Value, Vec<String>) {
        self.sent.last().cloned().expect("something was sent")
    }

    fn start(&mut self) -> bool {
        let out = self.theatre.start(self.clock);
        let accepted = out.accepted;
        self.take(out);
        accepted
    }

    fn add(&mut self, text: &str) {
        let out = self.theatre.add_link(text.into(), self.clock);
        self.take(out);
    }

    fn add_ticket(&mut self, ticket: &str) -> bool {
        let out = self.theatre.add_ticket(ticket.into(), None, self.clock);
        let accepted = out.accepted;
        self.take(out);
        accepted
    }

    fn ready(&mut self, key: &str) {
        let out = self.theatre.player_ready(key.into(), self.clock);
        self.take(out);
    }

    fn ended(&mut self, key: &str) {
        let out = self.theatre.player_ended(key.into(), self.clock);
        self.take(out);
    }

    fn play(&mut self) {
        let out = self.theatre.play(self.clock);
        self.take(out);
    }

    fn pause(&mut self) {
        let out = self.theatre.pause(self.clock);
        self.take(out);
    }

    fn seek(&mut self, to: f64) {
        let out = self.theatre.seek(to, self.clock);
        self.take(out);
    }

    fn skip(&mut self, key: &str) {
        let out = self.theatre.skip(Some(key.into()), self.clock);
        self.take(out);
    }

    fn close(&mut self) {
        let out = self.theatre.close(self.clock);
        self.take(out);
    }

    fn tick(&mut self) {
        let out = self.theatre.tick(self.clock);
        self.take(out);
    }

    /// A first video added and playable at the holder.
    fn playing(&mut self, title: &str) {
        self.add(&format!("https://example.invalid/{title}"));
        let key = self.key();
        self.ready(&key);
    }
}

fn close_to(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}

mod the_show {
    use super::*;

    #[test]
    fn plays_the_first_video_added_and_queues_the_rest_and_moves_on_when_one_ends() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.playing("one");
        room.add("https://example.invalid/two");

        let show = room.show().unwrap();
        assert!(show.playing);
        assert_eq!(show.position, 0.0);
        assert_eq!(show.now.unwrap().ticket, ticket_of("one"));
        assert_eq!(show.queue.iter().map(|one| one.ticket.clone()).collect::<Vec<_>>(), vec![ticket_of("two")]);

        room.ended("not-this-one");
        assert_eq!(room.now().unwrap().ticket, ticket_of("one"));
        let key = room.key();
        room.ended(&key);
        assert_eq!(room.now().unwrap().ticket, ticket_of("two"));
    }

    #[test]
    fn keeps_time_while_playing_holds_still_while_paused_and_goes_where_it_is_sent() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.playing("one");

        room.clock += 10_000;
        assert!(close_to(room.position(), 10.0));

        room.pause();
        room.clock += 5_000;
        assert!(!room.playing_now());
        assert!(close_to(room.position(), 10.0));

        room.seek(42.0);
        assert_eq!(room.position(), 42.0);
        room.seek(1e6);
        assert_eq!(room.position(), 100.0);
        room.seek(-5.0);
        assert_eq!(room.position(), 0.0);
    }

    #[test]
    fn waits_at_the_start_until_the_holder_can_play_the_video_so_everybody_begins_together() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.add("https://example.invalid/one");

        room.clock += 3000;
        assert!(!room.playing_now());
        assert_eq!(room.position(), 0.0);

        room.ready("some-other-video");
        assert!(!room.playing_now());
        let key = room.key();
        room.ready(&key);
        room.clock += 2000;
        assert!(room.playing_now());
        assert!(close_to(room.position(), 2.0));
    }

    #[test]
    fn starts_without_the_holders_player_if_it_never_can_play() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.add("https://example.invalid/one");

        room.clock += STARTING_MS + 1000;

        assert!(room.playing_now());
        assert!(close_to(room.position(), 1.0));
    }

    #[test]
    fn counts_every_change_so_a_late_message_is_known_to_be_late() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.add("https://example.invalid/one");
        let before = room.show().unwrap().version;

        room.pause();
        room.play();
        room.seek(3.0);

        assert_eq!(room.show().unwrap().version, before + 3);
    }

    #[test]
    fn does_as_anybody_in_the_call_asks_and_tells_only_the_asker_a_link_could_not_be_added() {
        let mut room = Room::new("gholder-1", &["gfriend-2", "gother-3"]);
        room.start();

        room.send("gfriend-2", json!({"t": "add", "text": "https://example.invalid/one"}));
        let now = room.now().unwrap();
        assert_eq!((now.ticket.as_str(), now.by.as_str()), (ticket_of("one").as_str(), "GFRIEND-2"));
        let key = room.key();
        room.ready(&key);

        room.send("gother-3", json!({"t": "pause"}));
        assert!(!room.playing_now());
        room.send("gother-3", json!({"t": "seek", "to": 12}));
        assert_eq!(room.position(), 12.0);
        room.send("gother-3", json!({"t": "play"}));
        assert!(room.playing_now());

        room.send("gother-3", json!({"t": "add", "text": "https://example.invalid/bad"}));
        assert_eq!(room.last(), (json!({"t": "refused", "why": "not_a_video"}), vec!["gother-3".to_string()]));
        assert!(room.refused.is_empty());
    }

    #[test]
    fn reads_a_link_somebody_else_pasted_as_somebody_elses_and_its_own_as_its_own() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();

        room.send("gfriend-2", json!({"t": "add", "text": "https://example.invalid/theirs"}));
        room.add("https://example.invalid/mine");
        room.add("https://example.invalid/also-mine");

        assert_eq!(
            room.resolved,
            vec![
                ("https://example.invalid/theirs".to_string(), true),
                ("https://example.invalid/mine".to_string(), false),
                ("https://example.invalid/also-mine".to_string(), false),
            ]
        );
    }

    #[test]
    fn queues_a_ticket_somebody_else_read_once_the_server_says_what_it_plays_and_reads_nothing_itself() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();

        room.send("gfriend-2", json!({"t": "add", "ticket": ticket_of("film")}));

        assert_eq!(room.read, vec![ticket_of("film")]);
        assert!(room.resolved.is_empty());
        let now = room.now().unwrap();
        assert_eq!((now.ticket, now.by.as_str(), now.duration), (ticket_of("film"), "GFRIEND-2", 300.0));
    }

    /// A media server's film is relayed on the grant of whoever chose it, and
    /// its ticket is in the show everybody in the call is told. Queued from
    /// anybody, a ticket copied out of one room and handed to the holder of
    /// another was relayed to that room on the first person's grant. So a film
    /// the server names a grantor for is queued only from that grantor, by the
    /// signature in their identity.
    #[test]
    fn queues_a_relayed_film_only_from_whoever_s_grant_it_is_relayed_under() {
        let granted = "aeeeeefffff-1";
        let copier = "tggggghhhhh-2";
        let guest = "geeeeefffff-3";
        let mut room = Room::new("gholder-1", &[granted, copier, guest]);
        room.reading = Box::new(|ticket: &str| WatchAnswer::Found {
            ticket: ticket.to_string(),
            duration: 300.0,
            live: false,
            grantor: Some("eeeeefffff".into()),
        });
        room.start();

        // Somebody else, proven, handing over the granted person's film.
        room.send(copier, json!({"t": "add", "ticket": ticket_of("film")}));
        assert_eq!(room.last(), (json!({"t": "refused", "why": "emby_not_allowed"}), vec![copier.to_string()]));
        assert!(room.now().is_none());

        // A guest whose signature happens to read the same is not anybody's grant.
        room.send(guest, json!({"t": "add", "ticket": ticket_of("film")}));
        assert_eq!(room.last(), (json!({"t": "refused", "why": "emby_not_allowed"}), vec![guest.to_string()]));
        assert!(room.now().is_none());

        // The person whose grant it is.
        room.send(granted, json!({"t": "add", "ticket": ticket_of("film")}));
        assert_eq!(room.now().unwrap().ticket, ticket_of("film"));
    }

    #[test]
    fn takes_the_ticket_over_the_link_where_a_message_carries_both_and_reads_nothing_itself() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();

        room.send("gfriend-2", json!({"t": "add", "ticket": ticket_of("film"), "text": "https://example.invalid/film"}));

        assert_eq!(room.read, vec![ticket_of("film")]);
        assert!(room.resolved.is_empty());
        assert_eq!(room.now().unwrap().ticket, ticket_of("film"));
    }

    /// A bangumi episode handed over after the switch was turned off, or a
    /// film whose grant was withdrawn: the server read it, and will not relay
    /// it. Said to the sender as a video that cannot be played here, which is
    /// what it is; "could not be read" had them try it again.
    #[test]
    fn tells_the_sender_a_ticket_the_server_will_not_relay_cannot_be_played_here() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.reading = Box::new(|_| WatchAnswer::Refused { why: "unavailable".into() });
        room.start();

        room.send("gfriend-2", json!({"t": "add", "ticket": ticket_of("episode")}));

        assert_eq!(room.last(), (json!({"t": "refused", "why": "unavailable"}), vec!["gfriend-2".to_string()]));
        assert!(room.now().is_none());
    }

    #[test]
    fn tells_only_the_sender_a_ticket_could_not_be_added_and_never_asks_about_one_that_is_not_a_ticket() {
        let mut room = Room::new("gholder-1", &["gfriend-2", "gother-3"]);
        room.start();

        room.send("gfriend-2", json!({"t": "add", "ticket": "expired_or_refused_ticket"}));
        assert_eq!(room.last(), (json!({"t": "refused", "why": "failed"}), vec!["gfriend-2".to_string()]));

        for ticket in [json!("https://evil.example.invalid/v.mp4"), json!("short"), json!(42), json!(null)] {
            room.send("gother-3", json!({"t": "add", "ticket": ticket}));
            assert_eq!(room.last(), (json!({"t": "refused", "why": "failed"}), vec!["gother-3".to_string()]));
        }

        assert_eq!(room.read.len(), 1);
        assert!(room.resolved.is_empty());
        assert!(room.now().is_none());
        assert!(room.refused.is_empty());
    }

    #[test]
    fn adds_a_ticket_of_its_own_the_same_way_and_says_here_when_it_will_not_read() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();

        room.add_ticket(&ticket_of("film"));
        assert_eq!(room.now().unwrap().ticket, ticket_of("film"));
        assert_eq!(room.read, vec![ticket_of("film")]);

        room.add_ticket("expired_or_refused_ticket");
        assert_eq!(room.refused, vec!["failed".to_string()]);
    }

    #[test]
    fn tells_the_asker_a_media_servers_film_was_not_theirs_to_add_and_never_that_the_holders_permission_is_theirs() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.resolve = Box::new(|text: &str| WatchAnswer::Refused {
            why: if text.starts_with("library:") { "emby_not_allowed" } else { "not_allowed" }.into(),
        });
        room.start();

        room.send("gfriend-2", json!({"t": "add", "text": "library:home/1a2b"}));
        assert_eq!(room.last(), (json!({"t": "refused", "why": "emby_not_allowed"}), vec!["gfriend-2".to_string()]));

        room.send("gfriend-2", json!({"t": "add", "text": "https://example.invalid/one"}));
        assert_eq!(room.last(), (json!({"t": "refused", "why": "unavailable"}), vec!["gfriend-2".to_string()]));

        // Its own, said here as what it is.
        room.add("https://example.invalid/one");
        assert_eq!(room.refused, vec!["not_allowed".to_string()]);
    }

    #[test]
    fn tells_whoever_runs_it_here_when_its_own_link_could_not_be_added() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();

        room.add("https://example.invalid/bad");

        assert_eq!(room.refused, vec!["not_a_video".to_string()]);
    }

    #[test]
    fn says_last_of_all_that_it_closed() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.add("https://example.invalid/one");

        room.close();
        room.play();

        assert_eq!(room.last().0, json!({"t": "closed"}));
    }

    #[test]
    fn gives_its_own_show_the_same_view_until_the_show_changes() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.add("https://example.invalid/one");

        room.clock += 3000;
        let first = room.theatre.state(room.clock);
        room.clock += 3000;
        assert_eq!(room.theatre.state(room.clock).heard_at_ms, first.heard_at_ms);
        assert_eq!(room.theatre.state(room.clock).show, first.show);

        room.pause();
        assert_ne!(room.theatre.state(room.clock).show, first.show);
    }

    #[test]
    fn skips_only_the_video_it_was_asked_to_skip_so_two_presses_do_not_skip_two() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.playing("one");
        room.add("https://example.invalid/two");
        room.add("https://example.invalid/three");
        let first = room.key();

        room.skip(&first);
        room.skip(&first);

        assert_eq!(room.now().unwrap().ticket, ticket_of("two"));
    }

    #[test]
    fn moves_on_from_a_video_the_holders_player_never_finished_once_the_timeline_is_past_its_end() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.add("https://example.invalid/one");
        room.add("https://example.invalid/two");

        // Never ready and never ended: a holder who cannot play it at all.
        room.clock += STARTING_MS + 100_000;
        room.tick();
        assert_eq!(room.now().unwrap().ticket, ticket_of("one"));

        room.clock += (OVERRUN_S * 1000.0) as u64;
        room.tick();
        assert_eq!(room.now().unwrap().ticket, ticket_of("two"));

        // Paused past the end is still paused, not over.
        room.clock += STARTING_MS + 100_000;
        room.pause();
        room.clock += 60_000;
        for _ in 0..5 {
            room.clock += TELL_EVERY;
            room.tick();
        }
        assert_eq!(room.now().unwrap().ticket, ticket_of("two"));
    }

    #[test]
    fn keeps_telling_the_call_it_is_there_while_paused_more_often_than_a_viewer_gives_up_on_it() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.playing("one");
        room.pause();
        let states = |room: &Room| room.sent.iter().filter(|(message, _)| message["t"] == "state").count();
        let before = states(&room);

        for _ in 0..STALE_MS / TELL_EVERY {
            room.clock += TELL_EVERY;
            room.tick();
        }

        let told = states(&room) - before;
        assert!(told as u64 >= STALE_MS / TELL_STILL - 1, "told {told} times in {STALE_MS} ms");
    }

    #[test]
    fn says_where_it_is_every_two_seconds_while_it_plays() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.playing("one");
        let before = room.sent.len();

        for _ in 0..5 {
            room.clock += TELL_EVERY;
            room.tick();
        }

        assert_eq!(room.sent.len() - before, 5);
    }

    #[test]
    fn tells_somebody_arriving_and_somebody_saying_hello_where_it_is() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.playing("one");

        let before = room.sent.len();
        let out = room.theatre.participant_connected("gnew-3".into(), room.clock);
        room.take(out);
        room.send("gfriend-2", json!({"t": "hello"}));

        assert_eq!(room.sent.len() - before, 2);
        assert!(room.sent[before..].iter().all(|(message, to)| message["t"] == "state" && to.is_empty()));
    }
}

mod heard_from_elsewhere {
    use super::*;

    fn state(version: u64, position: f64) -> Value {
        json!({
            "holder": "gholder-2",
            "holderName": "",
            "queue": [],
            "playing": true,
            "position": position,
            "version": version,
            "now": {"key": "k", "by": "b", "ticket": ticket_of("x"), "duration": 100, "live": false},
        })
    }

    fn listening(others: &[&str]) -> Room {
        let mut room = Room::new("gme-1", others);
        let out = room.theatre.joined(room.clock);
        room.take(out);
        room
    }

    #[test]
    fn asks_on_arrival_where_the_show_is() {
        let room = listening(&["gholder-2"]);
        assert_eq!(room.sent, vec![(json!({"t": "hello"}), Vec::<String>::new())]);
    }

    #[test]
    fn is_believed_from_whoever_runs_it_and_an_older_word_after_a_newer_one_changes_nothing() {
        let mut room = listening(&["gholder-2", "gliar-3"]);

        room.send("gliar-3", json!({"t": "state", "show": state(9, 0.0)}));
        assert!(room.show().is_none());

        room.send("gholder-2", json!({"t": "state", "show": state(5, 50.0)}));
        room.send("gholder-2", json!({"t": "state", "show": state(4, 10.0)}));
        assert_eq!(room.show().unwrap().position, 50.0);
        // The holder's own name, as the room knows it, over what the show says.
        assert_eq!(room.show().unwrap().holder_name, "GHOLDER-2");
    }

    #[test]
    fn is_not_believed_once_whoever_runs_it_closes_it_or_leaves() {
        let mut room = listening(&["gholder-2"]);

        room.send("gholder-2", json!({"t": "state", "show": state(1, 0.0)}));
        room.send("gholder-2", json!({"t": "closed"}));
        assert!(room.show().is_none());

        room.send("gholder-2", json!({"t": "state", "show": state(2, 0.0)}));
        assert!(room.show().is_some());
        let out = room.theatre.participant_disconnected("gholder-2".into(), room.clock);
        assert!(out.changed);
        room.take(out);
        assert!(room.show().is_none());
    }

    #[test]
    fn is_not_believed_once_its_holder_is_no_longer_in_the_call_though_their_leaving_went_unheard() {
        let mut room = listening(&["gholder-2", "gother-3"]);
        room.send("gholder-2", json!({"t": "state", "show": state(1, 0.0)}));

        // A reconnect replaces the roster without saying who left.
        let out = room.theatre.roster(vec!["gother-3".into()], room.clock);
        assert!(out.changed);
        room.take(out);

        assert!(room.show().is_none());
    }

    #[test]
    fn is_forgotten_when_its_holder_has_said_nothing_for_twenty_seconds() {
        let mut room = listening(&["gholder-2"]);
        room.send("gholder-2", json!({"t": "state", "show": state(1, 0.0)}));

        room.clock += STALE_MS;
        assert!(room.show().is_some());
        room.clock += 1;
        assert!(room.show().is_none());
        // And the screen is told, so it does not go on drawing a show that ended.
        assert!(room.theatre.tick(room.clock).changed);
    }

    #[test]
    fn is_ignored_when_what_arrives_is_not_a_show_and_stripped_of_anything_a_show_does_not_carry() {
        let mut room = listening(&["gholder-2"]);

        let mut bad = state(1, 0.0);
        bad["queue"] = json!("not a list");
        room.send("gholder-2", json!({"t": "state", "show": bad}));
        assert!(room.show().is_none());

        let mut extra = state(2, 0.0);
        extra["now"]["url"] = json!("https://evil.example.invalid/v.mp4");
        room.send("gholder-2", json!({"t": "state", "show": extra}));
        let now = room.now().unwrap();
        assert_eq!(now, WatchVideo { key: "k".into(), by: "b".into(), ticket: ticket_of("x"), duration: 100.0, live: false });
    }

    #[test]
    fn is_driven_by_asking_whoever_runs_it_and_nobody_else() {
        let mut room = listening(&["gholder-2", "gother-3"]);
        room.send("gholder-2", json!({"t": "state", "show": state(1, 0.0)}));

        room.pause();

        assert_eq!(room.last(), (json!({"t": "pause"}), vec!["gholder-2".to_string()]));
    }

    #[test]
    fn hears_why_a_link_it_sent_was_refused() {
        let mut room = listening(&["gholder-2"]);
        room.send("gholder-2", json!({"t": "state", "show": state(1, 0.0)}));

        room.send("gholder-2", json!({"t": "refused", "why": "unavailable"}));

        assert_eq!(room.refused, vec!["unavailable".to_string()]);
    }

    #[test]
    fn hears_a_refusal_that_names_a_permission_as_that_and_one_it_does_not_know_as_a_failure() {
        let mut room = listening(&["gholder-2", "gliar-3"]);
        room.send("gholder-2", json!({"t": "state", "show": state(1, 0.0)}));

        room.send("gholder-2", json!({"t": "refused", "why": "emby_not_allowed"}));
        room.send("gholder-2", json!({"t": "refused", "why": "not_allowed"}));
        room.send("gholder-2", json!({"t": "refused", "why": "<b>anything</b>"}));
        // Why a video cannot be played, where the gateway said: a members'
        // Bilibili episode pasted by a guest is that to the guest too.
        room.send("gholder-2", json!({"t": "refused", "why": "members_only"}));
        room.send("gholder-2", json!({"t": "refused", "why": "region_locked"}));
        room.send("gholder-2", json!({"t": "refused", "why": "not_found"}));
        // Only whoever runs the show says why something was not added to it.
        room.send("gliar-3", json!({"t": "refused", "why": "emby_not_allowed"}));

        assert_eq!(
            room.refused,
            ["emby_not_allowed", "not_allowed", "failed", "members_only", "region_locked", "not_found"]
                .map(String::from)
                .to_vec()
        );
    }

    #[test]
    fn hands_whoever_runs_it_a_ticket_and_nothing_that_ticket_stands_for() {
        let mut room = listening(&["gholder-2"]);
        room.send("gholder-2", json!({"t": "state", "show": state(1, 0.0)}));

        room.add_ticket(&ticket_of("film"));

        assert_eq!(room.last(), (json!({"t": "add", "ticket": ticket_of("film")}), vec!["gholder-2".to_string()]));
    }

    #[test]
    fn hands_over_the_link_beside_the_ticket_for_a_holder_older_than_tickets() {
        let mut room = listening(&["gholder-2"]);
        room.send("gholder-2", json!({"t": "state", "show": state(1, 0.0)}));

        let out = room.theatre.add_ticket(ticket_of("film"), Some("https://example.invalid/film".into()), room.clock);
        room.take(out);

        assert_eq!(
            room.raw.last().unwrap().clone(),
            format!(r#"{{"t":"add","ticket":"{}","text":"https://example.invalid/film"}}"#, ticket_of("film"))
        );
    }

    #[test]
    fn keeps_a_second_show_from_being_started_beside_it() {
        let mut room = listening(&["gholder-2"]);
        room.send("gholder-2", json!({"t": "state", "show": state(1, 0.0)}));

        assert!(!room.start());
    }

    #[test]
    fn starts_a_show_here_for_a_ticket_whose_show_ended_while_the_link_was_being_read() {
        let mut room = listening(&["gholder-2"]);

        assert!(room.add_ticket(&ticket_of("film")));

        assert!(room.theatre.state(room.clock).mine);
        assert_eq!(room.now().unwrap().ticket, ticket_of("film"));
    }

    #[test]
    fn asks_nobody_and_takes_nothing_where_there_is_no_show() {
        let mut room = listening(&["gholder-2"]);
        let before = room.sent.len();

        let out = room.theatre.pause(room.clock);
        assert!(!out.accepted);
        let out = room.theatre.add_link("https://example.invalid/one".into(), room.clock);
        assert!(!out.accepted);
        room.take(out);

        assert_eq!(room.sent.len(), before);
    }
}

#[test]
fn says_and_hears_nothing_in_a_sealed_call() {
    let mut room = Room::sealed("gme-1", &["gholder-2"], true);
    let out = room.theatre.joined(room.clock);
    room.take(out);

    room.send(
        "gholder-2",
        json!({"t": "state", "show": {"holder": "gholder-2", "holderName": "", "queue": [], "playing": false, "position": 0, "version": 1}}),
    );

    assert!(!room.start());
    assert!(room.show().is_none());
    assert!(!room.add_ticket(&ticket_of("film")));
    assert!(room.sent.is_empty());
    assert!(!room.theatre.state(room.clock).offered);
}

mod what_is_heard_of_a_show {
    use super::*;

    fn good() -> Value {
        json!({
            "holder": "gholder-2",
            "holderName": "H",
            "queue": [{"key": "k2", "by": "b", "ticket": ticket_of("two"), "duration": 5, "live": false}],
            "playing": true,
            "position": 3,
            "version": 2,
            "now": {"key": "k1", "by": "b", "ticket": ticket_of("one"), "duration": 9, "live": false},
        })
    }

    fn video(key: &str, title: &str, duration: f64) -> WatchVideo {
        WatchVideo { key: key.into(), by: "b".into(), ticket: ticket_of(title), duration, live: false }
    }

    #[test]
    fn is_taken_as_it_is_when_every_field_is_what_it_should_be() {
        assert_eq!(
            read_show(&good()),
            Some(WatchShow {
                holder: "gholder-2".into(),
                holder_name: "H".into(),
                now: Some(video("k1", "one", 9.0)),
                queue: vec![video("k2", "two", 5.0)],
                playing: true,
                position: 3.0,
                version: 2,
            })
        );
    }

    #[test]
    fn is_dropped_whole_when_any_of_it_is_not_rather_than_taking_a_page_down() {
        let with = |field: &str, value: Value| {
            let mut show = good();
            show[field] = value;
            show
        };
        let mut bad_queue_ticket = good();
        bad_queue_ticket["queue"][0]["ticket"] = json!("javascript:alert(1)");
        let mut bad_now_ticket = good();
        bad_now_ticket["now"]["ticket"] = json!("https://evil.example.invalid/x.mp4");
        let mut no_version = good();
        no_version.as_object_mut().unwrap().remove("version");
        let long = Value::Array(vec![good()["queue"][0].clone(); 501]);

        for bad in [
            with("queue", json!("not a list")),
            bad_queue_ticket,
            bad_now_ticket,
            with("position", json!(-1)),
            // NaN cannot be written in JSON; a string is the nearest a sender gets.
            with("position", json!("NaN")),
            with("playing", json!("yes")),
            no_version,
            with("queue", long),
            // `null` is not "no video" in the web, where only absent is.
            with("now", json!(null)),
            with("now", json!({"key": "x".repeat(65), "by": "b", "ticket": ticket_of("one")})),
            json!(null),
            json!("state"),
        ] {
            assert_eq!(read_show(&bad), None, "{bad}");
        }
    }

    #[test]
    fn carries_no_address_title_or_picture_which_come_from_the_server_by_the_ticket() {
        let mut extra = good();
        extra["now"]["url"] = json!("https://evil.example.invalid/v.mp4");
        extra["now"]["title"] = json!("x");
        extra["now"]["cover"] = json!("https://evil/c.jpg");
        assert_eq!(read_show(&extra).unwrap().now, Some(video("k1", "one", 9.0)));
    }

    #[test]
    fn keeps_a_name_to_a_hundred_units_and_a_missing_length_as_unknown() {
        let mut long = good();
        long["holderName"] = json!("n".repeat(150));
        long["now"]["by"] = json!("b".repeat(150));
        long["now"].as_object_mut().unwrap().remove("duration");
        let show = read_show(&long).unwrap();
        assert_eq!(show.holder_name.len(), 100);
        assert_eq!(show.now.as_ref().unwrap().by.len(), 100);
        assert_eq!(show.now.unwrap().duration, 0.0);
    }
}

/*
 * What crosses the call, byte for byte. A browser parses either way; these
 * hold the native client to the bytes a browser in its place would send, so a
 * capture of the two side by side shows nothing to tell them apart.
 */
mod on_the_wire {
    use super::*;

    fn text(room: &Room, at: usize) -> String {
        room.raw[at].clone()
    }

    #[test]
    fn says_a_show_as_json_stringify_does() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        assert_eq!(
            text(&room, 0),
            r#"{"t":"state","show":{"holder":"gholder-1","holderName":"Holder","queue":[],"playing":false,"position":0,"version":0}}"#
        );
        let out = room.theatre.add_link("https://example.invalid/one".into(), room.clock);
        let WatchAsk::Resolve { request, .. } = out.asks[0].clone() else { panic!("asked to read the link") };
        let out = room.theatre.answer(request, found(ticket_of("one"), 100.0), room.clock);
        let payload = String::from_utf8(out.packets[0].payload.clone()).unwrap();
        let key = room.key();
        assert_eq!(
            payload,
            format!(
                r#"{{"t":"state","show":{{"holder":"gholder-1","holderName":"Holder","now":{{"key":"{key}","by":"Holder","ticket":"{}","duration":100,"live":false}},"queue":[],"playing":false,"position":0,"version":1}}}}"#,
                ticket_of("one")
            )
        );
    }

    #[test]
    fn makes_keys_the_way_the_web_does() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.clock = 1_700_000_000_000;
        room.start();
        room.add("https://example.invalid/one");
        room.add("https://example.invalid/two");
        let show = room.show().unwrap();
        assert_eq!(show.now.unwrap().key, "loyw3v28-0");
        assert_eq!(show.queue[0].key, "loyw3v28-1");
    }

    #[test]
    fn asks_the_holder_in_the_webs_words() {
        let mut room = Room::new("gme-1", &["gholder-2"]);
        room.send(
            "gholder-2",
            json!({"t": "state", "show": {"holder": "gholder-2", "holderName": "", "queue": [], "playing": true, "position": 0, "version": 1,
                "now": {"key": "k", "by": "b", "ticket": ticket_of("x"), "duration": 100, "live": false}}}),
        );
        let payload = |out: WatchOutput| String::from_utf8(out.packets[0].payload.clone()).unwrap();

        assert_eq!(payload(room.theatre.play(room.clock)), r#"{"t":"play"}"#);
        assert_eq!(payload(room.theatre.seek(12.0, room.clock)), r#"{"t":"seek","to":12}"#);
        assert_eq!(payload(room.theatre.seek(12.5, room.clock)), r#"{"t":"seek","to":12.5}"#);
        assert_eq!(payload(room.theatre.skip(Some("k".into()), room.clock)), r#"{"t":"skip","key":"k"}"#);
        assert_eq!(payload(room.theatre.skip(None, room.clock)), r#"{"t":"skip"}"#);
        assert_eq!(payload(room.theatre.remove("k".into(), room.clock)), r#"{"t":"remove","key":"k"}"#);
        assert_eq!(payload(room.theatre.add_link("a link".into(), room.clock)), r#"{"t":"add","text":"a link"}"#);
    }

    #[test]
    fn reads_what_a_browser_sends_the_way_a_browser_holder_reads_it() {
        let mut room = Room::new("gholder-1", &["gfriend-2"]);
        room.start();
        room.playing("one");

        // Number("12") is twelve, and Number(null) is nought.
        room.send("gfriend-2", json!({"t": "seek", "to": "12"}));
        assert_eq!(room.position(), 12.0);
        room.send("gfriend-2", json!({"t": "seek", "to": null}));
        assert_eq!(room.position(), 0.0);
        // Number(undefined) is not a number, and is no seek.
        room.send("gfriend-2", json!({"t": "seek", "to": 30}));
        room.send("gfriend-2", json!({"t": "seek"}));
        assert_eq!(room.position(), 30.0);
        // A skip naming no string skips whatever is playing.
        room.add("https://example.invalid/two");
        room.send("gfriend-2", json!({"t": "skip", "key": 7}));
        assert_eq!(room.now().unwrap().ticket, ticket_of("two"));
        // Not JSON, or JSON without a type, is nothing.
        let out = room.theatre.on_message("gfriend-2".into(), String::new(), b"{not json".to_vec(), room.clock);
        assert!(out.packets.is_empty());
    }
}
