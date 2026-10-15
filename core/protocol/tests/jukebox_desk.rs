//! The song desk.
//!
//! The web client's own cases (web/src/live/jukebox.test.ts), ported one for
//! one. What would go wrong quietly: a song taken off the queue and never
//! played, two played at once, a skip that two impatient people turn into
//! skipping two songs, a request that lands under the wrong name, anything
//! said to the room in a sealed call, and a desk everybody still believes in
//! after its holder left.
//!
//! The web's tests give the desk fake dependencies that play nothing and
//! remember what they were asked; here the desk asks for the same things as
//! actions, and a small fake media side answers them the same way: at once,
//! except for a song held "slow" until released, and a play held at
//! publishing until the music is stopped, which then refuses it.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use serde_json::{json, Value};
use tomoshibi_protocol::jukebox::{
    desk_library, is_link, DeskAction, DeskEntry, DeskLibrary, DeskOutput, DeskPrepared, DeskSong, JukeboxDesk,
    ANNOUNCE_EVERY, STALE_MS, TOPIC,
};

fn song(id: &str) -> DeskSong {
    DeskSong {
        source: "lib".into(),
        id: id.into(),
        title: format!("Song {id}"),
        artists: vec!["Someone".into()],
        album: String::new(),
        cover: String::new(),
        duration: 200.0,
    }
}

fn song_json(id: &str) -> Value {
    json!({"source": "lib", "id": id, "title": format!("Song {id}"), "artists": ["Someone"], "album": "", "cover": "", "duration": 200})
}

struct Room {
    desk: Arc<JukeboxDesk>,
    clock: u64,
    sent: Vec<(Value, Vec<String>)>,
    raw: Vec<String>,
    /// Titles played, in order, and each one's "ended by itself".
    played: Vec<String>,
    ends: Vec<String>,
    /// What was fetched, as source/id, and what was abandoned while held.
    fetched: Vec<String>,
    aborted: Vec<String>,
    stopped: usize,
    paused: Vec<bool>,
    skipped: Vec<DeskEntry>,
    searched: Vec<String>,
    unavailable: HashSet<String>,
    /// Songs whose fetch fails outright.
    failing: HashSet<String>,
    /// Songs held on their way until released, as a slow library holds them.
    slowed: HashSet<String>,
    held: HashMap<String, u64>,
    /// A play held at publishing until the music is stopped.
    slow_publish: HashSet<String>,
    publishing: Option<u64>,
}

impl Room {
    fn new(identity: &str, others: &[&str]) -> Self {
        Self::with(identity, others, false)
    }

    fn with(identity: &str, others: &[&str], sealed: bool) -> Self {
        let mut room = Self {
            desk: JukeboxDesk::new(identity.into(), "Holder".into(), sealed),
            clock: 1000,
            sent: Vec::new(),
            raw: Vec::new(),
            played: Vec::new(),
            ends: Vec::new(),
            fetched: Vec::new(),
            aborted: Vec::new(),
            stopped: 0,
            paused: Vec::new(),
            skipped: Vec::new(),
            searched: Vec::new(),
            unavailable: HashSet::new(),
            failing: HashSet::new(),
            slowed: HashSet::new(),
            held: HashMap::new(),
            slow_publish: HashSet::new(),
            publishing: None,
        };
        let out = room.desk.roster(others.iter().map(|one| one.to_string()).collect(), room.clock);
        room.take(out);
        room
    }

    fn take(&mut self, out: DeskOutput) {
        for packet in out.packets {
            assert_eq!(packet.topic, TOPIC);
            assert!(packet.reliable);
            self.sent.push((serde_json::from_slice(&packet.payload).unwrap(), packet.destinations));
            self.raw.push(String::from_utf8(packet.payload).unwrap());
        }
        self.skipped.extend(out.skipped);
        for action in out.actions {
            let answer = match action {
                DeskAction::Prepare { request, entry } => {
                    if self.unavailable.contains(&entry.id) {
                        Some(self.desk.prepared(request, DeskPrepared::Unavailable, self.clock))
                    } else if self.failing.contains(&entry.id) {
                        Some(self.desk.prepared(request, DeskPrepared::Failed, self.clock))
                    } else {
                        self.fetched.push(format!("{}/{}", entry.source, entry.id));
                        if self.slowed.contains(&entry.id) {
                            self.held.insert(entry.id, request);
                            None
                        } else {
                            Some(self.desk.prepared(request, DeskPrepared::Ready { quality: Some("FLAC".into()) }, self.clock))
                        }
                    }
                }
                DeskAction::Abandon { request } => {
                    let held = self.held.iter().find(|(_, held)| **held == request).map(|(id, _)| id.clone());
                    if let Some(id) = held {
                        self.held.remove(&id);
                        self.aborted.push(id);
                    }
                    None
                }
                DeskAction::Play { request, entry } => {
                    if self.slow_publish.contains(&entry.title) {
                        self.publishing = Some(request);
                        None
                    } else {
                        self.played.push(entry.title);
                        self.ends.push(entry.key);
                        Some(self.desk.played(request, true, self.clock))
                    }
                }
                DeskAction::Stop => {
                    self.stopped += 1;
                    self.publishing.take().map(|request| self.desk.played(request, false, self.clock))
                }
                DeskAction::Pause { paused } => {
                    self.paused.push(paused);
                    None
                }
                DeskAction::Search { request, query, .. } => {
                    let hits = vec![song(&format!("found-{query}")), song("second")];
                    self.searched.push(query);
                    Some(self.desk.found(request, hits, self.clock))
                }
                DeskAction::ReadLink { request, .. } => {
                    Some(self.desk.found(request, vec![song("l1"), song("l2"), song("l3")], self.clock))
                }
            };
            if let Some(out) = answer {
                self.take(out);
            }
        }
    }

    fn release(&mut self, id: &str) {
        self.slowed.remove(id);
        if let Some(request) = self.held.remove(id) {
            let out = self.desk.prepared(request, DeskPrepared::Ready { quality: Some("FLAC".into()) }, self.clock);
            self.take(out);
        }
    }

    fn send(&mut self, from: &str, message: Value) {
        let out = self.desk.on_message(from.into(), from.to_uppercase(), message.to_string().into_bytes(), self.clock);
        self.take(out);
    }

    fn start(&mut self) -> bool {
        let out = self.desk.start(self.clock);
        let accepted = out.accepted;
        self.take(out);
        accepted
    }

    fn add(&mut self, ids: &[&str]) {
        let out = self.desk.add_songs(ids.iter().map(|id| song(id)).collect(), self.clock);
        self.take(out);
    }

    fn end_last(&mut self) {
        let key = self.ends.last().cloned().unwrap_or_default();
        self.end(&key);
    }

    fn end(&mut self, key: &str) {
        let out = self.desk.track_ended(key.into(), self.clock);
        self.take(out);
    }

    fn skip(&mut self) {
        let out = self.desk.skip(self.clock);
        self.take(out);
    }

    fn remove(&mut self, key: &str) {
        let out = self.desk.remove(key.into(), self.clock);
        self.take(out);
    }

    fn state(&self) -> tomoshibi_protocol::jukebox::DeskState {
        self.desk.state(self.clock).state.expect("a desk")
    }

    fn queue(&self) -> Vec<String> {
        self.state().queue.into_iter().map(|one| one.id).collect()
    }

    fn titles(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| format!("Song {name}")).collect()
    }
}

/// A desk running here, as the web's tests make one with `new Desk(...)`.
fn holding(others: &[&str]) -> Room {
    let mut room = Room::new("gholder-1", others);
    assert!(room.start());
    room
}

mod the_queue {
    use super::*;

    #[test]
    fn plays_the_first_song_added_and_the_next_when_it_ends_and_stops_when_there_is_none() {
        let mut room = holding(&["gfriend-2", "gother-3"]);

        room.add(&["a", "b"]);
        assert_eq!(room.played, Room::titles(&["a"]));
        assert_eq!(room.queue(), vec!["b"]);

        room.end_last();
        assert_eq!(room.played, Room::titles(&["a", "b"]));

        room.end_last();
        assert!(room.state().now.is_none());
        assert_eq!(room.stopped, 1);
    }

    #[test]
    fn fetches_the_next_song_while_this_one_plays() {
        let mut room = holding(&["gfriend-2", "gother-3"]);

        room.add(&["a", "b"]);

        assert_eq!(room.fetched, vec!["lib/a", "lib/b"]);
    }

    #[test]
    fn skips_a_song_the_account_may_not_play_says_so_and_plays_the_one_after() {
        let mut room = holding(&["gfriend-2", "gother-3"]);
        room.unavailable.insert("bad".into());

        room.add(&["bad", "good"]);

        assert_eq!(room.skipped.iter().map(|one| one.id.as_str()).collect::<Vec<_>>(), vec!["bad"]);
        assert_eq!(room.played, Room::titles(&["good"]));
    }

    #[test]
    fn moves_a_song_to_the_top_and_takes_one_out() {
        let mut room = holding(&["gfriend-2", "gother-3"]);
        room.add(&["a", "b", "c", "d"]);

        let queue = room.state().queue;
        let out = room.desk.to_top(queue[1].key.clone(), room.clock);
        room.take(out);
        room.remove(&queue[0].key);

        assert_eq!(room.queue(), vec!["c", "d"]);
    }

    #[test]
    fn tries_a_song_fetched_ahead_once_more_before_giving_it_up() {
        let mut room = holding(&["gfriend-2"]);
        room.add(&["a"]);
        room.failing.insert("b".into());
        room.add(&["b"]);
        room.failing.remove("b");

        room.end_last();

        assert_eq!(room.played, Room::titles(&["a", "b"]));
        assert!(room.skipped.is_empty());
    }

    #[test]
    fn gives_up_a_song_that_fails_ahead_and_again_and_plays_the_one_after() {
        let mut room = holding(&["gfriend-2"]);
        room.add(&["a"]);
        room.failing.insert("b".into());
        room.add(&["b", "c"]);

        room.end_last();

        assert_eq!(room.played, Room::titles(&["a", "c"]));
        assert_eq!(room.skipped.iter().map(|one| one.id.as_str()).collect::<Vec<_>>(), vec!["b"]);
    }
}

mod pausing {
    use super::*;

    #[test]
    fn pauses_what_is_playing_for_everybody_and_the_next_song_plays_unpaused() {
        let mut room = holding(&["gfriend-2", "gother-3"]);
        room.add(&["a", "b"]);

        let out = room.desk.pause(true, room.clock);
        room.take(out);
        assert_eq!(room.paused.last(), Some(&true));
        assert_eq!(room.sent.last().unwrap().0["state"]["paused"], json!(true));

        room.end_last();
        assert_eq!(room.state().now.unwrap().id, "b");
        assert!(!room.state().paused);
    }

    #[test]
    fn has_nothing_to_pause_when_nothing_is_playing() {
        let mut room = holding(&["gfriend-2", "gother-3"]);

        let out = room.desk.pause(true, room.clock);
        room.take(out);

        assert!(room.paused.is_empty());
        assert!(!room.state().paused);
    }
}

mod asking_the_desk {
    use super::*;

    #[test]
    fn plays_the_top_result_for_a_keyword_from_the_library_that_is_signed_in_under_the_askers_name() {
        let mut room = holding(&["gfriend-2", "gother-3"]);

        room.send("gfriend-2", json!({"t": "request", "query": "sunny day"}));

        assert_eq!(room.searched, vec!["sunny day"]);
        assert_eq!(room.played, vec!["Song found-sunny day"]);
        assert_eq!(room.state().now.unwrap().by, "GFRIEND-2");
        let libraries = vec![
            DeskLibrary { id: "off".into(), signed_in: false },
            DeskLibrary { id: "lib".into(), signed_in: true },
        ];
        assert_eq!(desk_library(libraries).as_deref(), Some("lib"));
        assert_eq!(desk_library(vec![DeskLibrary { id: "off".into(), signed_in: false }]).as_deref(), Some("off"));
        assert_eq!(desk_library(Vec::new()), None);
    }

    #[test]
    fn queues_every_song_a_pasted_link_names() {
        let mut room = holding(&["gfriend-2", "gother-3"]);

        room.send("gfriend-2", json!({"t": "link", "text": "a playlist https://example.invalid/list?id=1"}));

        assert_eq!(room.state().now.unwrap().id, "l1");
        assert_eq!(room.queue(), vec!["l2", "l3"]);
    }

    #[test]
    fn takes_songs_somebody_added_and_drops_what_is_not_a_song() {
        let mut room = holding(&["gfriend-2", "gother-3"]);

        let mut empty = song_json("");
        empty["id"] = json!("");
        room.send("gother-3", json!({"t": "add", "songs": [song_json("x"), {"nonsense": true}, empty]}));

        let now = room.state().now.unwrap();
        assert_eq!((now.id.as_str(), now.by.as_str()), ("x", "GOTHER-3"));
        assert!(room.state().queue.is_empty());
    }

    #[test]
    fn trims_a_keyword_and_searches_nothing_for_one_that_is_only_space() {
        let mut room = holding(&["gfriend-2"]);

        room.send("gfriend-2", json!({"t": "request", "query": "  sunny  "}));
        room.send("gfriend-2", json!({"t": "request", "query": "   "}));
        room.send("gfriend-2", json!({"t": "request", "query": "x".repeat(150)}));

        assert_eq!(room.searched, vec!["sunny".to_string(), "x".repeat(100)]);
    }

    #[test]
    fn asks_its_own_desk_by_link_or_keyword_and_says_when_nothing_came_of_it() {
        let room = holding(&["gfriend-2"]);

        let out = room.desk.ask("sunny".into(), room.clock);
        assert!(out.accepted);
        assert!(matches!(&out.actions[0], DeskAction::Search { mine: true, .. }));
        let out = room.desk.ask("see HTTPS://example.invalid/list".into(), room.clock);
        assert!(matches!(&out.actions[0], DeskAction::ReadLink { mine: true, .. }));
        let DeskAction::ReadLink { request, .. } = out.actions[0].clone() else { unreachable!() };
        assert!(!room.desk.found(request, Vec::new(), room.clock).accepted);
        assert!(is_link("http://x") && !is_link("sunny day"));
    }
}

mod skipping {
    use super::*;

    #[test]
    fn needs_a_majority_of_everybody_else_and_one_vote_each() {
        let mut room = holding(&["g2", "g3", "g4"]);
        room.add(&["a", "b"]);

        room.send("g2", json!({"t": "vote"}));
        room.send("g2", json!({"t": "vote"}));
        assert_eq!(room.played, Room::titles(&["a"]));
        assert_eq!(room.state().votes, vec!["g2"]);

        room.send("g3", json!({"t": "vote"}));
        assert_eq!(room.played, Room::titles(&["a", "b"]));
        assert!(room.state().votes.is_empty());
    }

    #[test]
    fn skips_the_song_on_its_way_when_the_holder_presses_again_while_it_loads() {
        let mut room = holding(&["gfriend-2", "gother-3"]);
        room.slowed.insert("b".into());
        room.add(&["a", "b", "c"]);

        room.skip();
        room.skip();

        assert_eq!(room.played, Room::titles(&["a", "c"]));
        assert!(room.aborted.contains(&"b".to_string()));
        assert!(room.state().queue.is_empty());
    }

    #[test]
    fn takes_out_the_song_on_its_way_to_playing_which_shows_as_loading_and_not_in_the_queue() {
        let mut room = holding(&["gfriend-2", "gother-3"]);
        room.slowed.insert("b".into());
        room.add(&["a", "b", "c"]);

        room.end_last();
        assert_eq!(room.state().next.unwrap().id, "b");
        assert!(room.state().loading);
        assert_eq!(room.queue(), vec!["c"]);

        let key = room.state().next.unwrap().key;
        room.remove(&key);

        assert!(room.aborted.contains(&"b".to_string()));
        assert_eq!(room.played, Room::titles(&["a", "c"]));
        assert!(room.skipped.is_empty());
    }

    #[test]
    fn lets_a_song_that_ends_by_itself_while_the_next_is_loading_leave_the_next_alone() {
        let mut room = holding(&["gfriend-2", "gother-3"]);
        room.slowed.insert("b".into());
        room.add(&["a", "b", "c"]);

        room.skip();
        let first = room.ends[0].clone();
        room.end(&first);
        room.release("b");

        assert!(!room.aborted.contains(&"b".to_string()));
        assert_eq!(room.played, Room::titles(&["a", "b"]));
    }

    #[test]
    fn abandons_a_song_taken_out_while_it_is_being_published_and_plays_the_next() {
        let mut room = holding(&["gfriend-2", "gother-3"]);
        room.slow_publish.insert("Song b".into());
        room.add(&["a", "b", "c"]);

        room.end_last();
        let loading = room.state().next.unwrap();
        assert_eq!(loading.id, "b");
        room.remove(&loading.key);

        assert!(room.stopped > 0);
        assert_eq!(room.played, Room::titles(&["a", "c"]));
        assert!(room.skipped.is_empty());
    }

    #[test]
    fn does_not_count_votes_cast_while_the_next_song_is_loading_which_were_against_the_one_already_skipped() {
        let mut room = holding(&["g2"]);
        room.slowed.insert("b".into());
        room.add(&["a", "b", "c"]);

        room.send("g2", json!({"t": "vote"}));
        room.send("g2", json!({"t": "vote"}));
        room.release("b");

        assert_eq!(room.played, Room::titles(&["a", "b"]));
        assert_eq!(room.queue(), vec!["c"]);
    }

    #[test]
    fn counts_the_majority_of_whoever_is_still_here() {
        let mut room = holding(&["g2", "g3", "g4", "g5"]);
        room.add(&["a", "b"]);
        assert_eq!(room.state().needed, 2);

        room.send("g2", json!({"t": "vote"}));
        let out = room.desk.participant_disconnected("g2".into(), room.clock);
        room.take(out);
        assert!(room.state().votes.is_empty());
        let out = room.desk.participant_disconnected("g3".into(), room.clock);
        room.take(out);
        assert_eq!(room.state().needed, 1);

        room.send("g4", json!({"t": "vote"}));
        assert_eq!(room.played, Room::titles(&["a", "b"]));
    }
}

mod telling_the_room {
    use super::*;

    #[test]
    fn says_nothing_at_all_in_a_sealed_call_and_hears_nothing() {
        let mut room = Room::with("gholder-1", &["gfriend-2", "gother-3"], true);
        assert!(room.start());

        room.add(&["a"]);
        room.send("gfriend-2", json!({"t": "add", "songs": [song_json("x")]}));

        assert!(room.sent.is_empty());
        assert!(room.state().queue.is_empty());
        assert_eq!(room.played, Room::titles(&["a"]));
        assert!(!room.desk.state(room.clock).broadcast);
    }

    #[test]
    fn says_last_of_all_that_it_closed_so_nobody_believes_in_it_afterwards() {
        let mut room = holding(&["gfriend-2", "gother-3"]);
        room.add(&["a"]);

        let out = room.desk.close(room.clock);
        room.take(out);

        assert_eq!(room.sent.last().unwrap().0, json!({"t": "closed"}));
        assert!(room.desk.state(room.clock).state.is_none());
    }

    #[test]
    fn announces_what_is_playing_and_what_is_queued() {
        let mut room = holding(&["gfriend-2", "gother-3"]);

        room.add(&["a", "b"]);

        let (last, to) = room.sent.last().unwrap().clone();
        assert!(to.is_empty());
        assert_eq!(last["t"], "state");
        assert_eq!(last["state"]["now"]["id"], "a");
        assert_eq!(last["state"]["queue"].as_array().unwrap().iter().map(|one| one["id"].clone()).collect::<Vec<_>>(), vec![json!("b")]);
    }

    #[test]
    fn says_it_is_there_every_fifteen_seconds_and_to_whoever_arrives_or_says_hello() {
        let mut room = holding(&["gfriend-2"]);
        let before = room.sent.len();

        room.clock += ANNOUNCE_EVERY - 1;
        let out = room.desk.tick(room.clock);
        room.take(out);
        assert_eq!(room.sent.len(), before);
        room.clock += 1;
        let out = room.desk.tick(room.clock);
        room.take(out);
        assert_eq!(room.sent.len(), before + 1);

        let out = room.desk.participant_connected("gnew-3".into(), room.clock);
        room.take(out);
        room.send("gfriend-2", json!({"t": "hello"}));
        assert_eq!(room.sent.len(), before + 3);
        assert_eq!(room.state().needed, 1);
    }

    #[test]
    fn stops_the_music_once_when_put_away_while_a_song_is_being_published() {
        let mut room = holding(&["gfriend-2"]);
        room.slow_publish.insert("Song a".into());
        room.add(&["a"]);

        let out = room.desk.close(room.clock);
        assert_eq!(out.actions.iter().filter(|action| matches!(action, DeskAction::Stop)).count(), 1);
        room.take(out);
        assert!(room.played.is_empty());
    }

    #[test]
    fn does_not_take_a_late_answer_to_a_desk_put_away_for_one_asked_by_the_next() {
        let mut room = holding(&["gfriend-2"]);
        room.slowed.insert("a".into());
        room.add(&["a"]);
        let late = room.held["a"];
        let out = room.desk.close(room.clock);
        room.take(out);

        assert!(room.start());
        room.slowed.insert("z".into());
        room.add(&["z"]);
        let out = room.desk.prepared(late, DeskPrepared::Ready { quality: None }, room.clock);
        room.take(out);

        assert!(room.played.is_empty());
        assert_eq!(room.state().next.unwrap().id, "z");
    }
}

mod heard_from_elsewhere {
    use super::*;

    fn state() -> Value {
        json!({"t": "state", "state": {"holder": "gholder-2", "holderName": "", "queue": [], "votes": [], "needed": 1, "loading": false}})
    }

    fn listening(others: &[&str]) -> Room {
        let mut room = Room::new("gme-1", others);
        let out = room.desk.joined(room.clock);
        room.take(out);
        room
    }

    fn heard(room: &Room) -> Option<String> {
        room.desk.state(room.clock).state.map(|state| state.holder)
    }

    #[test]
    fn asks_on_arrival_what_is_playing() {
        let room = listening(&["gholder-2"]);
        assert_eq!(room.raw, vec![r#"{"t":"hello"}"#]);
    }

    #[test]
    fn is_believed_while_its_holder_announces_it_and_not_once_they_leave_or_close_it() {
        let mut room = listening(&["gholder-2"]);

        room.send("gholder-2", state());
        assert_eq!(heard(&room).as_deref(), Some("gholder-2"));

        room.send("gholder-2", json!({"t": "closed"}));
        assert_eq!(heard(&room), None);

        room.send("gholder-2", state());
        let out = room.desk.participant_disconnected("gholder-2".into(), room.clock);
        room.take(out);
        assert_eq!(heard(&room), None);
    }

    #[test]
    fn is_not_believed_once_its_holder_is_no_longer_in_the_call_though_their_leaving_went_unheard() {
        let mut room = listening(&["gholder-2"]);
        room.send("gholder-2", state());

        // A reconnect replaces the roster without saying who left.
        let out = room.desk.roster(Vec::new(), room.clock);
        assert!(out.changed);
        room.take(out);

        assert_eq!(heard(&room), None);
    }

    #[test]
    fn is_forgotten_when_its_holder_has_said_nothing_for_a_long_while() {
        let mut room = listening(&["gholder-2"]);
        room.send("gholder-2", state());

        room.clock += 40_000;
        assert_eq!(heard(&room).as_deref(), Some("gholder-2"));
        room.clock += 10_000;
        assert_eq!(heard(&room), None);
        assert!(room.clock - 1000 > STALE_MS);
    }

    #[test]
    fn is_not_believed_when_somebody_announces_a_desk_held_by_somebody_else() {
        let mut room = listening(&["gholder-2", "gliar-3"]);

        room.send("gliar-3", state());

        assert_eq!(heard(&room), None);
    }

    #[test]
    fn keeps_a_second_desk_from_being_started_beside_it() {
        let mut room = listening(&["gholder-2"]);
        room.send("gholder-2", state());

        assert!(!room.start());
    }

    #[test]
    fn asks_whoever_holds_it_and_nobody_else_in_the_webs_words() {
        let mut room = listening(&["gholder-2", "gother-3"]);
        room.send("gholder-2", state());
        let to = vec!["gholder-2".to_string()];

        let out = room.desk.add_songs(vec![song("x")], room.clock);
        room.take(out);
        let out = room.desk.ask("sunny day".into(), room.clock);
        room.take(out);
        let out = room.desk.ask(format!("{} https://example.invalid/list", "y".repeat(3000)), room.clock);
        room.take(out);
        let out = room.desk.vote(room.clock);
        room.take(out);

        assert!(room.sent[1..].iter().all(|(_, sent_to)| *sent_to == to));
        assert_eq!(
            room.raw[1],
            r#"{"t":"add","songs":[{"source":"lib","id":"x","title":"Song x","artists":["Someone"],"album":"","cover":"","duration":200}]}"#
        );
        assert_eq!(room.raw[2], r#"{"t":"request","query":"sunny day"}"#);
        assert_eq!(room.sent[3].0["t"], "link");
        assert_eq!(room.sent[3].0["text"].as_str().unwrap().len(), 2000);
        assert_eq!(room.raw[4], r#"{"t":"vote"}"#);
    }

    #[test]
    fn asks_nobody_where_nobody_runs_a_desk() {
        let room = listening(&["gother-3"]);

        assert!(!room.desk.vote(room.clock).accepted);
        assert!(!room.desk.ask("sunny".into(), room.clock).accepted);
        assert!(!room.desk.add_songs(vec![song("x")], room.clock).accepted);
        assert_eq!(room.sent.len(), 1);
    }

    #[test]
    fn reads_a_heard_state_as_a_browser_draws_it_and_drops_a_song_that_is_not_one() {
        let mut room = listening(&["gholder-2"]);
        let mut entry = song_json("a");
        entry["key"] = json!("k1");
        entry["by"] = json!("B");
        room.send(
            "gholder-2",
            json!({"t": "state", "state": {"holder": "gholder-2", "holderName": "H", "now": entry, "queue": [{"key": "k2"}, entry],
                "votes": ["g9", 4], "needed": 2, "loading": true, "paused": true, "quality": "FLAC"}}),
        );

        let state = room.desk.state(room.clock).state.unwrap();
        assert_eq!(state.holder_name, "GHOLDER-2");
        assert_eq!(state.now.unwrap().key, "k1");
        assert_eq!(state.queue.len(), 1);
        assert_eq!(state.votes, vec!["g9"]);
        assert_eq!((state.needed, state.loading, state.paused), (2, true, true));
        assert_eq!(state.quality.as_deref(), Some("FLAC"));
    }
}

#[test]
fn says_its_state_as_json_stringify_does() {
    let room = holding(&["gfriend-2"]);
    assert_eq!(
        room.raw[0],
        r#"{"t":"state","state":{"holder":"gholder-1","holderName":"Holder","queue":[],"votes":[],"needed":1,"loading":false,"paused":false}}"#
    );
}
