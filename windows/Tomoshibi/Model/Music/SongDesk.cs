using System.Diagnostics;
using System.Globalization;
using Microsoft.UI.Dispatching;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Media;
using Tomoshibi.UI.Room;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.Model.Music;

/// <summary>Which part of the panel is showing (MusicPanel.tsx <c>Tab</c>).</summary>
internal enum MusicTab { Desk, Search, Playlist }

/// <summary>
/// One call's song desk (Android's feature/music/SongDesk.kt;
/// MusicFeature.swift <c>SongDesk</c>; web/src/live/jukebox.ts, music.ts,
/// sound.ts, duck.ts, MusicPanel.tsx): the protocol crate's
/// <c>JukeboxDesk</c> fed from the call and its answers carried out, the
/// music it plays, and the music it hears turned down under speech.
///
/// The desk itself is the protocol crate's: this tells it what the call says
/// and does what it asks (fetch and decode a song, publish it, pause it, stop
/// it, search the library, read a link), answering with what came of each.
/// Everything it says to the room goes out on the <c>jukebox</c> topic
/// exactly as the web writes it. Every answer to the desk is given from a
/// task of its own, never from inside the handling of another of its
/// outputs: what one call asks must all be done, in order, before anything
/// the next one says.
///
/// Made once per call and kept in the call's parts. UI thread only.
/// </summary>
internal sealed class SongDesk : Observable, IBarOffer
{
    public const string Part = "music";
    /// <summary>The data topic (jukebox.ts <c>TOPIC</c>).</summary>
    public const string Topic = "jukebox";

    public readonly CallModel Call;
    private readonly JukeboxDesk _desk;
    private readonly DispatcherQueueTimer _ticker;
    private DispatcherQueueTimer? _ducking;
    private readonly MusicDuck _duck;

    public DeskState? State { get; private set; }
    public bool Mine { get; private set; }
    /// <summary>The libraries this participant can reach; null where there is none or they are not signed in, which is most people (Room.tsx <c>library</c>).</summary>
    public IReadOnlyList<MusicLibrary>? Libraries { get; private set; }
    /// <summary>An ask is being looked up in the library here.</summary>
    public bool Asking { get; private set; }
    /// <summary>The music published from here, playing or paused or between songs.</summary>
    public bool PlayingHere { get; private set; }
    /// <summary>The panel is on screen: the bar keeps its button after the music has stopped, so the button that opened the panel is there to close it.</summary>
    public bool PanelOpen
    {
        get => _panelOpen;
        set
        {
            _panelOpen = value;
            Notify();
        }
    }
    private bool _panelOpen;

    // What the panel was showing, kept for the call: a playlist somebody had
    // opened was gone the moment they looked at the desk otherwise
    // (MusicPanel.tsx `useRemembered`).
    public MusicTab Tab = MusicTab.Desk;
    public string SearchSource = "";
    public string SearchQuery = "";
    public bool Searching { get; private set; }
    public IReadOnlyList<MusicTrack>? SearchResults { get; private set; }
    public string? SearchProblem { get; private set; }
    public HashSet<string> SearchAdded = new();
    public string Pasted = "";
    public bool Reading { get; private set; }
    public MusicLink? Linked { get; private set; }
    public string? LinkProblem { get; private set; }
    public HashSet<string> LinkAdded = new();

    private Task _sending = Task.CompletedTask;
    private readonly Dictionary<ulong, CancellationTokenSource> _loads = new();
    private readonly Dictionary<ulong, DecodedSong> _prepared = new();
    private MusicOutput? _output;
    private AudioInput? _published;
    private MonitorSound? _monitor;
    private Task<MusicOutput>? _opening;
    /// <summary>Bumped by every stop, so that a track still being published when the music was stopped finds out and takes itself down (sound.ts <c>generations</c>).</summary>
    private int _generation;
    private double _lastDuck;
    private bool _wasFading;
    private ulong? _asked;
    private bool _over;

    private SongDesk(CallModel call)
    {
        Call = call;
        _desk = new JukeboxDesk(call.LocalIdentity, call.LocalName, call.Sealed);
        _duck = new MusicDuck(call.Choices?.String(MusicKeys.Duck) != "off");
        _monitorVolume = MusicKeys.MonitorLevel(call.Choices?.String(MusicKeys.Monitor));
        call.Listen(On);
        // Whoever is here already was announced while the call was being
        // joined, before this existed: the roster, and any music they are
        // playing, are taken up now.
        var now = Now();
        Apply(_desk.Roster(call.Identities.ToArray(), now));
        Apply(_desk.Joined(now));
        foreach (var person in call.People)
        {
            if (person.MusicTrack is { } track) Subscribed(person.Id, track);
        }
        Refresh();
        // The holder's word every fifteen seconds, and a desk heard of that
        // has gone quiet: the desk wants the time at least once a second.
        _ticker = DispatcherQueue.GetForCurrentThread().CreateTimer();
        _ticker.Interval = TimeSpan.FromSeconds(1);
        _ticker.Tick += (_, _) =>
        {
            if (_over) return;
            if (Call.Over.IsCancellationRequested)
            {
                End();
                return;
            }
            Apply(_desk.Tick(Now()));
        };
        _ticker.Start();
        _ = FindLibraries();
    }

    /// <summary>The call's desk, made the first time anybody asks for it, with what it offers the bar beside it.</summary>
    public static SongDesk Of(CallModel call)
    {
        if (call.Parts.TryGetValue(Part, out var made) && made is SongDesk desk) return desk;
        desk = new SongDesk(call);
        call.Parts[Part] = desk;
        call.Parts[IBarOffer.Music] = desk;
        return desk;
    }

    /// <summary>The wall clock the desk's keys and staleness are written in, as <c>Date.now()</c> is in jukebox.ts.</summary>
    private static ulong Now() => (ulong)DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();

    /// <summary>A clock that never jumps, for the duck, in milliseconds.</summary>
    private static double Clock() => Stopwatch.GetTimestamp() * 1000.0 / Stopwatch.Frequency;

    // These three are kept between calls under the web's names; a
    // rehearsal keeps them in memory alone (MemoryDefaults), so a test's
    // half volume never reaches the owner's next real call.

    /// <summary>The library's quality, this participant's own (preferences.ts <c>meet-live.music-quality</c>): lossless unless the highest was chosen.</summary>
    public MusicQuality Quality
    {
        get => Call.Choices?.String(MusicKeys.Quality) == "best" ? MusicQuality.Best : MusicQuality.Lossless;
        set
        {
            if (value == MusicQuality.Best) Call.Choices?.Set(MusicKeys.Quality, "best");
            else Call.Choices?.Remove(MusicKeys.Quality);
            Notify();
        }
    }

    /// <summary>Whether the music is turned down while anybody talks (duck.ts <c>meet-live.music-duck</c>, on unless turned off).</summary>
    public bool Ducks
    {
        get => _duck.Enabled;
        set
        {
            _duck.Enabled = value;
            if (value) Call.Choices?.Remove(MusicKeys.Duck);
            else Call.Choices?.Set(MusicKeys.Duck, "off");
            ApplyDuck(Clock());
            Notify();
        }
    }

    private float _monitorVolume;

    /// <summary>How loud the music playing from here is in this computer's own speakers (sound.ts <c>monitorVolume</c>). Only there: what is sent is the decoded file whatever this says.</summary>
    public float MonitorVolume
    {
        get => _monitorVolume;
        set
        {
            _monitorVolume = Math.Clamp(value, 0f, 1f);
            if (_monitorVolume == 1f) Call.Choices?.Remove(MusicKeys.Monitor);
            else Call.Choices?.Set(MusicKeys.Monitor, _monitorVolume.ToString(CultureInfo.InvariantCulture));
            if (_monitor != null) _monitor.Volume = _monitorVolume;
            Notify();
        }
    }

    /// <summary>
    /// Whether the music goes out with a lossless stream beside it, as the
    /// web's <c>playDecoded(room, decoded, rememberedLossless())</c>
    /// decides: where it is wanted, and never in a sealed call, whose data
    /// is not sealed and would carry the file's samples across the relay in
    /// the clear. Any participant here can send it, which in the web only
    /// Chromium can.
    /// </summary>
    public bool LosslessOffered => TomoshibiProtocolMethods.SoundLosslessOffered(Call.Choices?.String(MusicKeys.Lossless) != "off", Call.Sealed, true);

    /// <summary>The libraries, asked once, as Room.tsx asks: a deployment without one, or somebody not signed in, is no library and no tabs to search it.</summary>
    private async Task FindLibraries()
    {
        MusicLibrary[] found;
        try
        {
            found = await Call.Deployment.MusicSources();
        }
        catch (Exception)
        {
            found = Array.Empty<MusicLibrary>();
        }
        if (_over) return;
        Libraries = found.Length > 0 ? found : null;
        if (SearchSource.Length == 0) SearchSource = (found.FirstOrDefault(l => l.SignedIn) ?? found.FirstOrDefault())?.Id ?? "";
        Notify();
    }

    // The call

    private void On(CallEvent e)
    {
        if (_over) return;
        var now = Now();
        switch (e)
        {
            case CallEvent.ParticipantJoined joined:
                Apply(_desk.ParticipantConnected(joined.Participant.Identity, now));
                break;
            case CallEvent.ParticipantLeft left:
                Apply(_desk.ParticipantDisconnected(left.Identity, now));
                _duck.Left(left.Identity);
                break;
            // A reconnect replaces the roster without saying who left.
            case CallEvent.StateChanged { State: CallState.Connected }:
                Apply(_desk.Roster(Call.Identities.ToArray(), now));
                break;
            case CallEvent.Data data:
                // Only from somebody in the room: the server's own messages
                // are not the desk's (jukebox.ts `onData`, `!participant`).
                if (data.Identity is not { } sender || data.Topic != Topic) return;
                Apply(_desk.OnMessage(sender, Call.PersonOf(sender)?.Name ?? "", data.Payload, now));
                break;
            case CallEvent.TrackPublished published when IsMusic(published.Track):
                Arrived(published.Identity);
                Notify();
                break;
            case CallEvent.TrackSubscribed subscribed:
                Subscribed(subscribed.Identity, subscribed.Track);
                break;
            case CallEvent.TrackUnpublished:
            case CallEvent.TrackUnsubscribed:
                // Whoever no longer plays music is no longer faded in.
                foreach (var person in Call.People.Where(p => p.MusicTrack == null)) _duck.Left(person.Id);
                Notify();
                break;
            case CallEvent.Ended:
                End();
                break;
        }
    }

    /// <summary>A remote music stream began to play here: faded in and turned down under speech.</summary>
    private void Subscribed(string identity, TrackInfo track)
    {
        if (!IsMusic(track)) return;
        Arrived(identity);
        var factor = (float)_duck.Factor(identity, Clock());
        Call.SetDuck(track.Sid, factor);
        Call.Lossless.SetDuck(identity, factor);
        Notify();
    }

    /// <summary>Music arriving from somebody whose desk is not heard here, which still has a volume (MusicPanel.tsx <c>player</c>).</summary>
    public Person? Player => State == null ? Call.People.FirstOrDefault(p => p.MusicTrack != null) : null;

    /// <summary>
    /// What the bar offers (MusicFeature.swift <c>available</c>,
    /// <c>playing</c>; Room.tsx ~280): the panel to somebody who can reach the
    /// library, before there is any music; to everybody once there is a desk
    /// or music to turn down or ask for; and while the panel is open.
    /// </summary>
    public bool Offered => Libraries != null || State != null || PlayingHere || _panelOpen || Call.People.Any(p => p.MusicTrack != null);
    public bool Playing => State?.Now != null || PlayingHere || Player != null;

    private void End()
    {
        if (_over) return;
        _over = true;
        _ticker.Stop();
        _ducking?.Stop();
        foreach (var load in _loads.Values) load.Cancel();
        _loads.Clear();
        foreach (var song in _prepared.Values) song.Dispose();
        _prepared.Clear();
        _output?.Stop();
        _output = null;
        if (_monitor != null) Call.Speakers?.Detach(_monitor);
    }

    // The desk's words and wishes

    private void Apply(DeskOutput output)
    {
        if (_over) return;
        foreach (var packet in output.Packets) Send(packet);
        foreach (var action in output.Actions) Perform(action);
        foreach (var entry in output.Skipped) Say(T("Skipped {title}: the library's account cannot play it.", ("title", entry.Title)));
        if (output.Changed) Refresh();
    }

    /// <summary>One sender, so the room hears the desk's words in the order it said them.</summary>
    private void Send(DeskPacket packet)
    {
        var previous = _sending;
        _sending = Next();

        async Task Next()
        {
            try
            {
                await previous;
            }
            catch (Exception)
            {
            }
            try
            {
                await Call.Send(packet.Topic, packet.Payload, packet.Destinations, packet.Reliable);
            }
            catch (Exception fault)
            {
                Log.Line($"music: a word did not go: {fault.Message}");
            }
        }
    }

    internal void Refresh()
    {
        var snapshot = _desk.State(Now());
        if (!Equals(snapshot.State, State) || snapshot.Mine != Mine)
        {
            State = snapshot.State;
            Mine = snapshot.Mine;
            Notify();
        }
    }

    private void Perform(DeskAction action)
    {
        switch (action)
        {
            case DeskAction.Prepare prepare:
                Prepare(prepare.Request, prepare.Entry);
                break;
            case DeskAction.Abandon abandon:
                if (_loads.Remove(abandon.Request, out var load)) load.Cancel();
                if (_prepared.Remove(abandon.Request, out var song)) song.Dispose();
                break;
            case DeskAction.Play play:
                _ = Play(play.Request, play.Entry);
                break;
            case DeskAction.Stop:
                StopMusic();
                break;
            case DeskAction.Pause pause:
                _output?.Pause(pause.Paused);
                break;
            case DeskAction.Search search:
                _ = Search(search.Request, search.Query, search.Mine);
                break;
            case DeskAction.ReadLink link:
                _ = ReadLink(link.Request, link.Text, link.Mine);
                break;
        }
    }

    /// <summary>Describe, fetch and decode, off the UI thread; the desk is told what came of it unless it abandoned the request meanwhile.</summary>
    private void Prepare(ulong request, DeskEntry entry)
    {
        var quality = Quality;
        // The file's own samples are kept only where a lossless stream will
        // carry them: the track already out says, or else whether the next
        // one would be offered.
        var keepOriginal = _output?.Lossless ?? LosslessOffered;
        var cancel = new CancellationTokenSource();
        _loads[request] = cancel;
        _ = Load();

        async Task Load()
        {
            DecodedSong? song = null;
            DeskPrepared outcome;
            try
            {
                var loaded = await Task.Run(() => MusicLoader.Load(Call.Deployment, entry.Source, entry.Id, quality, keepOriginal, cancel.Token), cancel.Token);
                if (loaded is MusicLoader.Outcome.Ready ready)
                {
                    song = ready.Song;
                    outcome = new DeskPrepared.Ready(song.Quality);
                }
                else
                {
                    outcome = new DeskPrepared.Unavailable();
                }
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (Exception fault)
            {
                Log.Line($"music: {entry.Title} could not be prepared: {fault.Message}");
                outcome = new DeskPrepared.Failed();
            }
            if (!_loads.Remove(request) || _over)
            {
                song?.Dispose();
                return;
            }
            if (song != null) _prepared[request] = song;
            Apply(_desk.Prepared(request, outcome, Now()));
        }
    }

    private async Task Play(ulong request, DeskEntry entry)
    {
        _prepared.Remove(request, out var song);
        var started = _generation;
        var ok = false;
        if (song != null)
        {
            // Any failure, a stop that overtook the publishing among them, is
            // a song that did not play, which the desk is told.
            MusicOutput? output = null;
            try
            {
                output = await OpenOutput();
            }
            catch (Exception fault)
            {
                Log.Line($"music: no track for {entry.Title}: {fault.Message}");
            }
            if (output != null && started == _generation && !_over)
            {
                output.Play(song, entry.Key);
                ok = true;
            }
        }
        if (!ok) song?.Dispose();
        if (_over) return;
        Apply(_desk.Played(request, ok, Now()));
    }

    /// <summary>The music's own track, published the first time a song plays and kept across the songs that follow until the music stops.</summary>
    private Task<MusicOutput> OpenOutput()
    {
        if (_output != null) return Task.FromResult(_output);
        return _opening ??= Open();

        async Task<MusicOutput> Open()
        {
            var started = _generation;
            try
            {
                var offered = LosslessOffered;
                var track = await Call.Core.PublishAudio(new SoundKind.LibraryMusic(offered));
                // Stopped while it was being published: taken down again
                // rather than left playing silence into the room (sound.ts,
                // "a stop that arrives in between used to be outrun").
                if (started != _generation || _over)
                {
                    try
                    {
                        await track.Unpublish();
                    }
                    catch (Exception)
                    {
                    }
                    throw new OperationCanceledException("the music stopped while it was being published");
                }
                // The holder's own speakers, where there are any; the music
                // still goes out where there are not (a muted launch).
                MonitorSound? monitor = null;
                if (Call.Speakers is { } speakers)
                {
                    monitor = new MonitorSound { Volume = _monitorVolume };
                    speakers.Attach(monitor);
                }
                var made = new MusicOutput(track, offered, monitor, SongEnded);
                _monitor = monitor;
                Arrived(MusicDuck.Local);
                if (monitor != null) monitor.Duck = (float)_duck.Factor(MusicDuck.Local, Clock());
                made.Start();
                _output = made;
                _published = track;
                PlayingHere = true;
                Notify();
                Log.Line($"music: published, lossless offered {offered}");
                return made;
            }
            finally
            {
                _opening = null;
            }
        }
    }

    private void StopMusic()
    {
        _generation += 1;
        _opening = null;
        _output?.Stop();
        if (_published is { } track) _ = Unpublish(track);
        _output = null;
        _published = null;
        if (_monitor != null) Call.Speakers?.Detach(_monitor);
        _monitor = null;
        PlayingHere = false;
        _duck.Left(MusicDuck.Local);
        Notify();

        static async Task Unpublish(AudioInput track)
        {
            try
            {
                await track.Unpublish();
            }
            catch (Exception)
            {
            }
        }
    }

    private void SongEnded(string key)
    {
        if (!_over) Apply(_desk.TrackEnded(key, Now()));
    }

    /// <summary>The top hit for a keyword, from the library the desk searches (<c>desk_library</c>: the first signed in, or else the first).</summary>
    private async Task Search(ulong request, string query, bool mine)
    {
        var library = TomoshibiProtocolMethods.DeskLibrary((Libraries ?? Array.Empty<MusicLibrary>()).Select(l => new DeskLibrary(l.Id, l.SignedIn)).ToArray());
        var songs = Array.Empty<DeskSong>();
        var failed = false;
        if (library != null)
        {
            try
            {
                songs = (await Call.Deployment.MusicSearch(library, query, 1)).Select(t => Song(t, library)).ToArray();
            }
            catch (Exception)
            {
                failed = true;
            }
        }
        if (_over) return;
        var output = _desk.Found(request, songs, Now());
        Apply(output);
        if (!mine) return;
        Answered(request, failed ? T("The library did not answer. Try again.") : output.Accepted ? null : T("Nothing found."));
    }

    private async Task ReadLink(ulong request, string text, bool mine)
    {
        var songs = Array.Empty<DeskSong>();
        var failed = false;
        try
        {
            if (await Call.Deployment.MusicLink(text) is { } link) songs = link.Tracks.Select(t => Song(t, link.Source)).ToArray();
        }
        catch (Exception)
        {
            failed = true;
        }
        if (_over) return;
        var output = _desk.Found(request, songs, Now());
        Apply(output);
        if (!mine) return;
        Answered(request, failed ? T("The library did not answer. Try again.") : output.Accepted ? null : T("That link names no playlist or song."));
    }

    private void Answered(ulong request, string? problem)
    {
        if (_asked == request)
        {
            _asked = null;
            Asking = false;
            Notify();
        }
        if (problem != null) Say(problem);
    }

    // What the person does

    /// <summary>Where nobody runs a desk and this participant can reach a library, it starts here first (MusicPanel.tsx <c>add</c>, <c>ask</c>).</summary>
    private void StartIfNobodyRunsOne()
    {
        if (State == null && Libraries != null) Apply(_desk.Start(Now()));
    }

    /// <summary>Adds songs chosen from the library: to the desk here, or sent to whoever runs the one heard.</summary>
    public void Add(IReadOnlyList<DeskSong> songs)
    {
        StartIfNobodyRunsOne();
        var output = _desk.AddSongs(songs.ToArray(), Now());
        Apply(output);
        if (!output.Accepted) Say(T("Nobody is running the song desk."));
    }

    /// <summary>Asks for a song by what was typed: a song's name, or a playlist or song link (MusicPanel.tsx <c>ask</c>).</summary>
    public void Ask(string typed)
    {
        var text = typed.Trim();
        if (text.Length == 0 || Asking) return;
        StartIfNobodyRunsOne();
        var output = _desk.Ask(text, Now());
        // The search or link this set off, where the desk is here: the box
        // stays busy until it is answered, as the web's awaits it.
        foreach (var action in output.Actions)
        {
            ulong? request = action switch
            {
                DeskAction.Search { Mine: true } search => search.Request,
                DeskAction.ReadLink { Mine: true } link => link.Request,
                _ => null,
            };
            if (request is not { } asked) continue;
            _asked = asked;
            Asking = true;
        }
        Apply(output);
        if (!output.Accepted) Say(Mine ? T("Nothing found.") : T("Nobody is running the song desk."));
        Notify();
    }

    public void Vote() => Apply(_desk.Vote(Now()));
    public void Skip() => Apply(_desk.Skip(Now()));
    public void Pause(bool paused) => Apply(_desk.Pause(paused, Now()));
    public void Remove(string key) => Apply(_desk.Remove(key, Now()));
    public void ToTop(string key) => Apply(_desk.ToTop(key, Now()));
    public void Close() => Apply(_desk.Close(Now()));

    // How loud somebody's music is here

    /// <summary>The listener's own setting for somebody's music: the one the sound panel keeps for that person's music, so a change in either is the other's too.</summary>
    public HearingSetting Setting(string identity) => Tomoshibi.Model.Hearing.Shared.Setting(identity, Sound.Music);

    /// <summary>Searches the library chosen in the panel's search tab.</summary>
    public async Task SearchLibrary()
    {
        var asked = SearchQuery.Trim();
        if (asked.Length == 0 || Searching) return;
        Searching = true;
        SearchProblem = null;
        Notify();
        try
        {
            SearchResults = await Call.Deployment.MusicSearch(SearchSource, asked, 1);
        }
        catch (Exception)
        {
            SearchProblem = T("The library did not answer. Try again.");
        }
        finally
        {
            Searching = false;
            Notify();
        }
    }

    public void ChooseLibrary(string id)
    {
        SearchSource = id;
        SearchResults = null;
        Notify();
    }

    /// <summary>Reads the link pasted in the panel's playlist tab.</summary>
    public async Task OpenLink()
    {
        if (string.IsNullOrWhiteSpace(Pasted) || Reading) return;
        Reading = true;
        LinkProblem = null;
        Notify();
        try
        {
            var found = await Call.Deployment.MusicLink(Pasted);
            if (found == null) LinkProblem = T("That link names no playlist or song.");
            Linked = found;
            LinkAdded = new HashSet<string>();
        }
        catch (Exception)
        {
            LinkProblem = T("The library did not answer. Try again.");
        }
        finally
        {
            Reading = false;
            Notify();
        }
    }

    /// <summary>Said in the call's corner, as the web says it (MusicPanel.tsx <c>actionFailed</c>): only inside the panel it went unseen whenever the panel was shut, which is when a song is skipped.</summary>
    private void Say(string text) => Call.ActionFailed(text);

    // Turning the music down under speech

    /// <summary>A music stream has started from somebody, or from here: faded in, and the microphones listened to while there is music to turn down.</summary>
    private void Arrived(string key)
    {
        _duck.Arrived(key, Clock());
        if (_ducking != null || _over) return;
        _lastDuck = Clock();
        _ducking = DispatcherQueue.GetForCurrentThread().CreateTimer();
        _ducking.Interval = TimeSpan.FromMilliseconds(MusicDuck.TickMs);
        _ducking.Tick += (_, _) =>
        {
            if (_over || !DuckTick())
            {
                _ducking?.Stop();
                _ducking = null;
            }
        };
        _ducking.Start();
    }

    /// <summary>
    /// One reading of every microphone that is on, this participant's
    /// included, and the music moved (duck.ts <c>tick</c>). False once there
    /// is no music and nothing still moving, when the clock stops. The levels
    /// are a tenth of a second's RMS each, where the web reads 1024 samples
    /// every 40 ms: a sound has to last a little longer here to count as
    /// speech.
    /// </summary>
    private bool DuckTick()
    {
        var now = Clock();
        var music = _output != null || _opening != null || Call.People.Any(p => p.MusicTrack != null);
        var levels = new List<double>();
        foreach (var person in Call.People)
        {
            if (person.Track(MediaSource.Microphone) is { Muted: false } microphone && Call.Level(microphone.Sid) is { } level) levels.Add(level);
        }
        if (music && Call.MicrophoneOn) levels.Add(Call.MicrophoneLevel);
        // Once more on the first tick after a fade-in ends: a tick that came
        // late left the music at whatever the last tick inside the fade had
        // made it (0.56 for good, measured on Apple).
        var fadedIn = _wasFading && !_duck.Fading(now);
        if (_duck.Tick(levels, now, now - _lastDuck) || fadedIn) ApplyDuck(now);
        _wasFading = _duck.Fading(now);
        _lastDuck = now;
        return music || !_duck.Settled(now);
    }

    private void ApplyDuck(double now)
    {
        foreach (var person in Call.People)
        {
            if (person.MusicTrack is not { } track) continue;
            var factor = (float)_duck.Factor(person.Id, now);
            Call.SetDuck(track.Sid, factor);
            // And the lossless stream standing in for it, played at the same
            // level (lossless-receiver.ts `levelOf`).
            Call.Lossless.SetDuck(person.Id, factor);
        }
        if (_monitor != null) _monitor.Duck = (float)_duck.Factor(MusicDuck.Local, now);
    }

    /// <summary>How far the music is turned down now.</summary>
    public double DuckLevel => _duck.Level;

    /// <summary>Where the song playing from here is and how long it is, for the holder's own panel; nobody else knows, since the desk does not say.</summary>
    public (double At, double Of)? Progress => _output?.Progress;

    /// <summary>The song desk's music, in either form (sound.ts <c>isMusic</c>): its own source, so that it and a screen's sound never take each other's place.</summary>
    public static bool IsMusic(TrackInfo track) =>
        track.Kind == MediaKind.Audio && track.Source == MediaSource.Unknown && track.Name is "music" or "music-lossless";

    public static DeskSong Song(MusicTrack track, string source) =>
        new(source, track.Id, track.Title, track.Artists, track.Album, track.Cover, track.Duration);
}
