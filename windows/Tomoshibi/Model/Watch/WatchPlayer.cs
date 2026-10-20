using System.Globalization;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Media;
using uniffi.tomoshibi_protocol;

namespace Tomoshibi.Model.Watch;

/// <summary>
/// One way of playing: Windows' media player for a file or a playlist, or
/// YouTube's own player in a web view (WatchScreen.tsx <c>Handle</c>;
/// Android's WatchBackend).
/// </summary>
internal interface IWatchBackend
{
    /// <summary>It has the video and can say where it is.</summary>
    bool Ready { get; }
    double Time { get; }
    /// <summary>Not paused, as a video element's <c>!paused &amp;&amp; !ended</c> reads: a player waiting for data it was told to play is playing.</summary>
    bool Playing { get; }
    double Rate { get; }
    /// <summary>It can run a little fast or slow; YouTube's cannot by small amounts.</summary>
    bool Nudges { get; }
    bool Youtube { get; }
    /// <summary>It has enough to go on playing: a video element's <c>readyState >= 3</c>.</summary>
    bool Flowing { get; }
    void Play();
    void Pause();
    void Seek(double to);
    void SetRate(double rate);
    void SetVolume(double level, bool muted);
    void Stop();
    /// <summary>Loads the same address again from where it stood: what hls.js's <c>startLoad</c> does for a relay that stopped.</summary>
    void Reload() { }
    /// <summary>YouTube's frame takes presses only while this is on.</summary>
    void SetTappable(bool on) { }
}

/// <summary>
/// The video everybody is watching, as this app plays it (Android's
/// feature/watch/WatchPlayer.kt; WatchScreen.tsx <c>Player</c>): one per
/// video, as the web keys its player by the video.
///
/// Each viewer plays the video themselves and keeps to the show's timeline.
/// Where it plays from is decided by trying: the place the video is, and if
/// that fails (an error, or a player that has still not begun
/// <see cref="StallMs"/> after it should have) the same video relayed by
/// the deployment. YouTube plays through YouTube's own player where YouTube
/// can be reached and through the relay otherwise. On a deployment that
/// relays nothing there is no second place, and a viewer whose connection
/// cannot reach the video is told so.
/// </summary>
internal sealed class WatchPlayer
{
    /// <summary>How often the player is held to the show (<c>KEEP_MS</c>).</summary>
    public const int KeepMs = 250;
    /// <summary>A player that has not begun this long after it should have is given the relay (<c>STALL_MS</c>).</summary>
    public const long StallMs = 8000;
    /// <summary>How many times a relay-only video that stopped partway is started again by itself before the viewer is asked to (<c>RESUMES</c>).</summary>
    public const int Resumes = 2;
    /// <summary>YouTube's errors for a video whose owner allows it on no other site.</summary>
    public static readonly int[] EmbeddingRefused = { 101, 150 };
    /// <summary>Where this viewer's volume is kept: the web's own key (WatchScreen.tsx <c>VOLUME_KEY</c>).</summary>
    public const string VolumeKey = "meet-live.watch-volume";

    /// <summary>Why a video does not play for this viewer: their connection, the site's refusal, or a kind of video this player cannot open at all.</summary>
    public enum Lost { Network, Refused, Failed, Format }

    /// <summary>Where a relay-only video that had been playing stands, once its relay stopped.</summary>
    public enum Halted { Resuming, Stopped }

    public readonly WatchVideo Video;
    public readonly ResolvedVideo Info;
    private readonly WatchEngine _engine;
    private readonly string _base;

    /// <summary>Playing the deployment's relayed copy rather than the original.</summary>
    public bool Relayed { get; private set; }
    /// <summary>YouTube's player was asked to play and did not: it wants a press inside the player itself.</summary>
    public bool Blocked { get; private set; }
    /// <summary>Not playing for this viewer, with no relay to fall back on.</summary>
    public Lost? Unreachable { get; private set; }
    public Halted? Stopping { get; private set; }
    /// <summary>Nothing to hold to yet: a spinner.</summary>
    public bool Waiting { get; private set; } = true;
    /// <summary>Where the clock under the video reads.</summary>
    public double Position { get; private set; }
    public IWatchBackend? Backend { get; private set; }

    private double _volume = 1;
    private bool _muted;

    private bool _played;
    private bool _triedDirect;
    private bool _triedRelay;
    private bool _unopenable;
    private int _resumed;
    private bool _askingAgain;
    private readonly WatchStall _stall = new(0, StallMs);
    private bool _gaveUp;
    private WatchKeeper _keeper = new();
    private bool _playingShow;
    private bool _stopped;

    public WatchPlayer(WatchVideo video, ResolvedVideo info, WatchEngine engine, string baseUrl, bool mainland)
    {
        Video = video;
        Info = info;
        _engine = engine;
        _base = baseUrl;
        if (engine.Call.Choices?.String(VolumeKey) is { } kept && double.TryParse(kept, NumberStyles.Float, CultureInfo.InvariantCulture, out var level) && double.IsFinite(level) && level is >= 0 and <= 1)
        {
            _volume = level;
        }
        // Straight to the relay, for a YouTube video and a viewer who came in
        // through a relay in mainland China, where the server relays it at
        // all (WatchScreen.tsx ~240): YouTube's script does not arrive from
        // there, and every such viewer sat at a black tile waiting for it to
        // fail before being given the relay they were always going to get.
        var straightToRelay = mainland && info.Play.Kind == ResolvedPlayKind.Youtube && info.Proxy != null;
        Relayed = info.Relay || straightToRelay;
        Build();
    }

    public double Volume
    {
        get => _volume;
        set
        {
            _volume = value;
            ApplyVolume();
            // Kept only when it is not the default, as the web keeps it.
            if (_volume == 1) _engine.Call.Choices?.Remove(VolumeKey);
            else _engine.Call.Choices?.Set(VolumeKey, _volume.ToString(CultureInfo.InvariantCulture));
            Changed();
        }
    }

    public bool Muted
    {
        get => _muted;
        set
        {
            _muted = value;
            ApplyVolume();
            Changed();
        }
    }

    private void Changed() => _engine.PlayerChanged();

    // Where it plays from

    private bool RelayOnly => Info.Play.Kind is ResolvedPlayKind.Relay or ResolvedPlayKind.Bangumi;

    public bool PlaysAsYoutube => Info.Play.Kind == ResolvedPlayKind.Youtube && !Relayed;

    /// <summary>The relay, unless this plays from where the video is; a relay-only source has nowhere else to play from.</summary>
    private (string Url, bool Hls)? Source
    {
        get
        {
            if (Relayed || Info.Play.Kind == ResolvedPlayKind.Youtube || RelayOnly)
            {
                return Info.Proxy is { } proxy ? (proxy.Url, proxy.Kind == ResolvedProxyKind.Hls) : null;
            }
            return Info.Play.Url is { } url ? (url, Info.Play.Kind == ResolvedPlayKind.Hls) : null;
        }
    }

    private void Build()
    {
        Backend?.Stop();
        Backend = null;
        _gaveUp = false;
        if (Relayed) _triedRelay = true;
        _stall.Restart(WatchWords.Now());
        _keeper = new WatchKeeper();
        Attach();
        Changed();
    }

    private void Attach()
    {
        var id = Info.Play.Id;
        if (PlaysAsYoutube && id != null)
        {
            Backend = new YouTubeBackend(id, _base, this, _engine.Call.Launch.WatchYoutubeApi, _engine.Muted);
        }
        else if (Source is { } source)
        {
            // Held to the size it is shown at where the relay is not the one
            // chosen for its quality: a film at four times the pixels the
            // tile has is the deployment's bandwidth spent on nothing anybody
            // sees (hls.js `capLevelToPlayerSize: !info.relay`).
            Backend = new FileBackend(source.Url, source.Hls, !Info.Relay, this, WatchCeilings.Stored(_engine.Call.Choices), _engine.Muted);
        }
        if (Backend != null)
        {
            _stall.Restart(WatchWords.Now());
            ApplyVolume();
            Log.Line($"watch: {(PlaysAsYoutube ? "youtube" : Relayed ? "relay" : "direct")} source for {Video.Key}");
        }
    }

    /// <summary>Where the video is has failed this viewer: the relay, or where there is none, saying so.</summary>
    private void GiveUp(Lost why = Lost.Network)
    {
        if (Info.Proxy != null)
        {
            if (!Relayed)
            {
                Relayed = true;
                Build();
            }
        }
        else
        {
            var was = Unreachable;
            Unreachable = was is { } before && before != Lost.Network ? before : why;
            Changed();
        }
    }

    /// <summary>
    /// A file or playlist that failed: given the other place once, and only
    /// while it has never played; said once neither plays. The second
    /// failure is the end: "this video does not reach you", with the
    /// sentence for why and the link to open it elsewhere (Android's reason:
    /// a player that opens no WebM spun for ever with nothing said).
    /// </summary>
    private void Failed(string why, bool format = false)
    {
        if (_played) return;
        if (format) _unopenable = true;
        var lost = _unopenable ? Lost.Format : Lost.Network;
        if (!Relayed)
        {
            if (_gaveUp) return;
            Log.Line($"watch: direct failed {why}");
            if (Info.Proxy != null && !_triedRelay)
            {
                Relayed = true;
                Build();
            }
            else
            {
                _gaveUp = true;
                Unreachable = lost;
                Changed();
            }
        }
        else if (Info.Relay && !RelayOnly && !_triedDirect)
        {
            // The relay was the first choice and failed: the lower quality
            // from where the video is, rather than nothing.
            Log.Line($"watch: relay failed {why}, trying direct");
            _triedDirect = true;
            Relayed = false;
            Build();
        }
        else
        {
            Log.Line($"watch: relay failed {why}, nowhere left to play it from");
            _gaveUp = true;
            Unreachable = lost;
            Changed();
        }
    }

    // What the backends report

    /// <summary>It can play: the holder's timeline starts now, for everybody.</summary>
    public void BackendPlayable(IWatchBackend from)
    {
        if (ReferenceEquals(from, Backend) && !_stopped) _engine.PlayerReady(Video.Key);
    }

    public void BackendBegan(IWatchBackend from)
    {
        if (!ReferenceEquals(from, Backend)) return;
        _played = true;
        _resumed = 0;
        if (Blocked)
        {
            Blocked = false;
            LetYoutubeTakePresses();
        }
        Unreachable = null;
        Stopping = null;
        Changed();
    }

    public void BackendEnded(IWatchBackend from)
    {
        if (ReferenceEquals(from, Backend) && !_stopped) _engine.PlayerEnded(Video.Key);
    }

    /// <summary>The player could not play it: before it has played, a source that does not work; after, for a video that plays only through the relay, the relay stopped.</summary>
    public void BackendFailed(IWatchBackend from, string why, bool format)
    {
        if (!ReferenceEquals(from, Backend)) return;
        if (_played && RelayOnly && !from.Youtube) _ = Resume(from);
        else Failed(why, format);
    }

    /// <summary>YouTube's script never arrived, or its player never became ready.</summary>
    public void YoutubeUnreachable(IWatchBackend from)
    {
        if (!ReferenceEquals(from, Backend)) return;
        Log.Line("watch: youtube unreachable");
        GiveUp();
    }

    /// <summary>YouTube's player refused the video: the relay, or where there is none, which of the two it was.</summary>
    public void YoutubeRefused(IWatchBackend from, int code)
    {
        if (!ReferenceEquals(from, Backend)) return;
        Log.Line($"watch: youtube refused {code}");
        GiveUp(EmbeddingRefused.Contains(code) ? Lost.Refused : Lost.Failed);
    }

    /// <summary>
    /// A relay-only video that stopped after it had played: the server is
    /// asked about the ticket again first, since a switch turned off or a
    /// grant withdrawn is no fault a retry cures; otherwise it is started
    /// again from where it stood, <see cref="Resumes"/> times, and then the
    /// viewer is told it stopped and given a press.
    /// </summary>
    private async Task Resume(IWatchBackend from)
    {
        if (_askingAgain) return;
        _askingAgain = true;
        Stopping = Halted.Resuming;
        Changed();
        var now = await _engine.AskAgain(Video.Ticket);
        _askingAgain = false;
        if (!ReferenceEquals(from, Backend) || now is WatchInfo.Unavailable) return;
        if (_resumed >= Resumes)
        {
            Stopping = Halted.Stopped;
            Changed();
            return;
        }
        _resumed += 1;
        from.Reload();
    }

    /// <summary>The press once it has stopped.</summary>
    public void TryAgain()
    {
        _resumed = 0;
        Stopping = Halted.Resuming;
        Backend?.Reload();
        Changed();
    }

    // Held to the show

    /// <summary>One keep step (WatchScreen.tsx <c>keep</c>, the rules in the protocol crate's <c>WatchKeeper</c>).</summary>
    public void Keep(long now, WatchShow show, long heardAtMs)
    {
        if (_stopped) return;
        _playingShow = show.Playing;
        if (Backend == null) Attach();
        CheckStall(now);
        var want = TomoshibiProtocolMethods.WatchExpected(show, (ulong)Math.Max(0, heardAtMs), (ulong)now);
        if (Backend is not { } backend)
        {
            SetWaiting(true);
            if (!Video.Live) SetPosition(want);
            return;
        }
        // A refused player is not asked again.
        var refusedForGood = Unreachable is Lost.Refused or Lost.Failed or Lost.Format;
        var reading = new WatchPlayerReading(backend.Ready && !refusedForGood, backend.Time, backend.Playing, backend.Rate, backend.Nudges, backend.Youtube);
        var step = _keeper.Keep(reading, show.Playing, want, Video.Live, (ulong)now);
        SetWaiting(step.Waiting);
        if (step.Play) backend.Play();
        if (step.Pause) backend.Pause();
        if (step.Seek is { } seek) backend.Seek(seek);
        if (step.Rate is { } rate) backend.SetRate(rate);
        if (step.Refused && !Blocked)
        {
            Blocked = true;
            LetYoutubeTakePresses();
            Changed();
        }
        if (step.Position is { } at) SetPosition(at);
    }

    private void SetWaiting(bool waiting)
    {
        if (Waiting == waiting) return;
        Waiting = waiting;
        Changed();
    }

    private void SetPosition(double at)
    {
        if (Math.Abs(at - Position) < 0.01) return;
        // Not a change worth redrawing the room for: the stage reads the
        // position on its own clock.
        Position = at;
    }

    /// <summary>Asked once a second, as the web's interval asks it.</summary>
    private void CheckStall(long now)
    {
        if (Backend is not { } backend || !_stall.Due(now)) return;
        var late = _stall.Late(now);
        if (backend.Youtube)
        {
            // The API arrived but the player never became ready: the relay.
            // A player that is ready and refused a press is the platform's
            // doing, and is waited on.
            if (!_played && _playingShow && late && !backend.Ready) YoutubeUnreachable(backend);
        }
        else
        {
            if (_played || (Relayed && !Info.Relay) || (Relayed && _triedDirect)) return;
            if (_playingShow && !backend.Flowing && late) Failed("stalled");
        }
    }

    // Pressed here

    public void PlayNow() => Backend?.Play();

    public void PauseNow() => Backend?.Pause();

    /// <summary>The viewer moved the player: counted as a jump, so the keeper does not jump it back before the show has heard the seek.</summary>
    public void SeekNow(double to)
    {
        _keeper.Seeked((ulong)WatchWords.Now());
        Backend?.Seek(to);
        SetPosition(to);
    }

    /// <summary>A refused YouTube player is started by a press inside YouTube's own frame, so the frame takes presses while it is refused.</summary>
    public bool TapYoutube => Blocked && PlaysAsYoutube;

    private void ApplyVolume()
    {
        Backend?.SetVolume(_volume, _muted);
        LetYoutubeTakePresses();
    }

    /// <summary>YouTube's frame takes no presses (the controls drive the show, and YouTube's own would drive only this player) except while it is refused.</summary>
    private void LetYoutubeTakePresses() => Backend?.SetTappable(TapYoutube);

    public void Stop()
    {
        _stopped = true;
        Backend?.Stop();
        Backend = null;
    }
}
