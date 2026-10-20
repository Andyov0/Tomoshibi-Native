using System.Globalization;
using System.Text.Json;
using System.Text.RegularExpressions;
using Microsoft.UI;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Web.WebView2.Core;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Model.Watch;
using Windows.Media.Core;
using Windows.Media.Playback;
using Windows.Media.Streaming.Adaptive;

namespace Tomoshibi.Media;

/// <summary>
/// A file or an HLS playlist, from where it is or relayed: Windows' media
/// player, as Media3's is on Android and AVPlayer on Apple (Android's
/// <c>ExoBackend</c>). A playlist goes through AdaptiveMediaSource, which
/// chooses among its renditions as hls.js does.
///
/// Kept by the video's player rather than by a view, so the picture survives
/// the stage being redrawn: the stage and the strip trading places only
/// lends the player another element to draw in.
/// </summary>
internal sealed class FileBackend : IWatchBackend
{
    public readonly MediaPlayer Player = new() { AutoPlay = false };
    private readonly string _url;
    private readonly bool _hls;
    private readonly bool _capped;
    private readonly WatchPlayer _owner;
    /// <summary>The viewer's own ceiling, read when the video is opened, as the web reads it ("a change on the account page applies from the next").</summary>
    private readonly WatchCeiling _ceiling;
    private readonly bool _silent;
    private readonly DispatcherQueue _ui;
    private AdaptiveMediaSource? _adaptive;
    /// <summary>The playlist's renditions by bitrate, read from its master playlist, which says their sizes where AdaptiveMediaSource says only their rates.</summary>
    private List<(uint Rate, WatchCeilings.Level Level)> _levels = new();
    private (int Width, int Height) _largest;
    /// <summary>What this app last told the player, so a reload keeps playing only what was playing.</summary>
    private bool? _asked;
    private bool _prepared;
    private bool _ended;
    private bool _gone;
    private static readonly HttpClient Fetch = new() { Timeout = TimeSpan.FromSeconds(15) };

    /// <summary>The picture's shape, width over height, for the stage to letterbox it.</summary>
    public double Aspect { get; private set; } = 16.0 / 9.0;
    public event Action? AspectChanged;

    public FileBackend(string url, bool hls, bool capped, WatchPlayer owner, WatchCeiling ceiling, bool silent)
    {
        _url = url;
        _hls = hls;
        _capped = capped;
        _owner = owner;
        _ceiling = ceiling;
        _silent = silent;
        _ui = DispatcherQueue.GetForCurrentThread();
        Player.IsMuted = silent;
        Player.MediaOpened += Opened;
        Player.MediaEnded += Ended;
        Player.MediaFailed += Failed;
        Player.PlaybackSession.PlaybackStateChanged += StateChanged;
        Player.PlaybackSession.NaturalVideoSizeChanged += SizeChanged;
        _ = Load(null);
    }

    // The player's events arrive on its own threads; everything the video's
    // player is told, it is told on the UI thread, as the model's rule is.
    private void OnUi(Action act) => _ui.TryEnqueue(() =>
    {
        if (!_gone) act();
    });

    private void Opened(MediaPlayer sender, object args) => OnUi(() =>
    {
        if (_prepared) return;
        _prepared = true;
        _owner.BackendPlayable(this);
    });

    private void Ended(MediaPlayer sender, object args) => OnUi(() =>
    {
        _ended = true;
        _owner.BackendEnded(this);
    });

    private void Failed(MediaPlayer sender, MediaPlayerFailedEventArgs args)
    {
        var error = args.Error;
        OnUi(() => _owner.BackendFailed(this, $"error {error}", Unopenable(error)));
    }

    private void StateChanged(MediaPlaybackSession sender, object args)
    {
        var state = sender.PlaybackState;
        OnUi(() =>
        {
            if (state == MediaPlaybackState.Playing) _owner.BackendBegan(this);
        });
    }

    private void SizeChanged(MediaPlaybackSession sender, object args)
    {
        var (width, height) = (sender.NaturalVideoWidth, sender.NaturalVideoHeight);
        OnUi(() =>
        {
            if (width == 0 || height == 0) return;
            Aspect = (double)width / height;
            AspectChanged?.Invoke();
        });
    }

    /// <summary>
    /// The errors for a file the player cannot open or decode, rather than
    /// one it could not fetch (Android's <c>unopenable</c>): no relay cures
    /// either.
    /// </summary>
    private static bool Unopenable(MediaPlayerError error) => error is MediaPlayerError.DecodingError or MediaPlayerError.SourceNotSupported;

    private async Task Load(double? at)
    {
        _prepared = false;
        _ended = false;
        MediaSource source;
        if (_hls)
        {
            // Said to be a playlist whatever its address looks like: the
            // relay's is `/api/watch/media?t=...`, with nothing in it that
            // names HLS.
            AdaptiveMediaSourceCreationResult made;
            try
            {
                made = await AdaptiveMediaSource.CreateFromUriAsync(new Uri(_url));
            }
            catch (Exception fault)
            {
                if (!_gone) _owner.BackendFailed(this, "playlist " + fault.Message, false);
                return;
            }
            if (_gone) return;
            if (made.Status != AdaptiveMediaSourceCreationStatus.Success)
            {
                var format = made.Status is AdaptiveMediaSourceCreationStatus.ManifestParseFailure
                    or AdaptiveMediaSourceCreationStatus.UnsupportedManifestContentType
                    or AdaptiveMediaSourceCreationStatus.UnsupportedManifestVersion
                    or AdaptiveMediaSourceCreationStatus.UnsupportedManifestProfile;
                _owner.BackendFailed(this, $"playlist {made.Status}", format);
                return;
            }
            _adaptive = made.MediaSource;
            await ReadLevels();
            if (_gone) return;
            Hold();
            source = MediaSource.CreateFromAdaptiveMediaSource(_adaptive);
        }
        else
        {
            source = MediaSource.CreateFromUri(new Uri(_url));
        }
        Player.Source = new MediaPlaybackItem(source);
        if (at is { } from)
        {
            Player.PlaybackSession.Position = TimeSpan.FromSeconds(from);
            if (_asked == true) Player.Play();
        }
    }

    /// <summary>The master playlist's renditions, ranked by bandwidth as hls.js ranks its levels; none for a playlist of one.</summary>
    private async Task ReadLevels()
    {
        if (_ceiling == WatchCeiling.Auto && !_capped) return;
        try
        {
            var text = await Fetch.GetStringAsync(_url);
            var found = new List<(uint, WatchCeilings.Level)>();
            foreach (Match m in Regex.Matches(text, @"#EXT-X-STREAM-INF:([^\r\n]*)"))
            {
                var attributes = m.Groups[1].Value;
                var bandwidth = Regex.Match(attributes, @"(?:^|,)BANDWIDTH=(\d+)");
                if (!bandwidth.Success) continue;
                var size = Regex.Match(attributes, @"RESOLUTION=(\d+)x(\d+)");
                double? width = size.Success ? double.Parse(size.Groups[1].Value, CultureInfo.InvariantCulture) : null;
                var height = size.Success ? double.Parse(size.Groups[2].Value, CultureInfo.InvariantCulture) : 0;
                found.Add((uint.Parse(bandwidth.Groups[1].Value, CultureInfo.InvariantCulture), new WatchCeilings.Level(width, height)));
            }
            _levels = found.OrderBy(l => l.Item1).ToList();
        }
        catch (Exception)
        {
            _levels = new();
        }
    }

    /// <summary>
    /// Holds a playlist to the viewer's ceiling and, where the relay is not
    /// the one chosen for its quality, to the size it is shown at (Android's
    /// <c>hold</c> and <c>shown</c>; hls.js's <c>autoLevelCapping</c> and
    /// <c>capLevelToPlayerSize</c>): the highest rendition each allows
    /// becomes the most this player may fetch, half way to the rendition
    /// above, as Apple and Android keep a margin under an exact limit. The
    /// lower of the two wins.
    /// </summary>
    private void Hold()
    {
        if (_adaptive == null || _levels.Count < 2) return;
        uint? most = null;
        if (_ceiling != WatchCeiling.Auto && _ceiling.Cap(_levels.Select(l => l.Level).ToList()) is { } cap) most = Margin(cap);
        if (_capped && _largest.Height > 0)
        {
            // The least rendition at least as large as the picture, as
            // hls.js chooses for a player of that size.
            var fits = _levels.FindIndex(l => (l.Level.Width ?? 0) >= _largest.Width || l.Level.Height >= _largest.Height);
            var sized = Margin(fits < 0 ? _levels.Count - 1 : fits);
            most = most is { } ceiling ? Math.Min(ceiling, sized) : sized;
        }
        if (most is not { } limit) return;
        _adaptive.DesiredMaxBitrate = limit;
        Log.Line($"watch: at most {limit} b/s of {string.Join(", ", _levels.Select(l => $"{l.Level.Height}p@{l.Rate}"))}");
    }

    private uint Margin(int index)
    {
        var allowed = _levels[index].Rate;
        var above = _levels.Skip(index + 1).Select(l => l.Rate).FirstOrDefault(r => r > allowed);
        if (above == 0) above = allowed * 2;
        return (uint)(((ulong)allowed + above) / 2);
    }

    /// <summary>
    /// The size the picture is shown at, in pixels, for a player held to it.
    /// Only ever grown: shrinking it while a larger rendition is buffered
    /// throws away what was fetched, and the stage and the strip trade
    /// places whenever somebody pins a picture.
    /// </summary>
    public void Shown(int width, int height)
    {
        if (!_capped || width <= 0 || height <= 0) return;
        var grown = (Math.Max(_largest.Width, width), Math.Max(_largest.Height, height));
        if (grown == _largest) return;
        _largest = grown;
        Hold();
    }

    public void Reload() => _ = Load(Time);

    public bool Ready => _prepared;
    public double Time => Math.Max(0, Player.PlaybackSession.Position.TotalSeconds);
    public bool Playing => _asked == true && !_ended;
    public double Rate => Player.PlaybackSession.PlaybackRate;
    public bool Nudges => true;
    public bool Youtube => false;
    public bool Flowing => _prepared && Player.PlaybackSession.PlaybackState is MediaPlaybackState.Playing or MediaPlaybackState.Paused;

    public void Play()
    {
        _asked = true;
        _ended = false;
        Player.Play();
    }

    public void Pause()
    {
        _asked = false;
        Player.Pause();
    }

    /// <summary>Exactly where asked, as a video element's <c>currentTime</c> is set: a seek that lands a keyframe away is a gap the keeper then spends seconds closing.</summary>
    public void Seek(double to) => Player.PlaybackSession.Position = TimeSpan.FromSeconds(Math.Max(0, to));

    public void SetRate(double rate)
    {
        if (Math.Abs(Player.PlaybackSession.PlaybackRate - rate) > 0.0001) Player.PlaybackSession.PlaybackRate = rate;
    }

    public void SetVolume(double level, bool muted)
    {
        Player.Volume = Math.Clamp(level, 0, 1);
        // A muted launch plays the video's sound at nought whatever the
        // viewer sets: a test hears nothing.
        Player.IsMuted = muted || _silent;
    }

    public void Stop()
    {
        if (_gone) return;
        _gone = true;
        Player.MediaOpened -= Opened;
        Player.MediaEnded -= Ended;
        Player.MediaFailed -= Failed;
        Player.PlaybackSession.PlaybackStateChanged -= StateChanged;
        Player.PlaybackSession.NaturalVideoSizeChanged -= SizeChanged;
        Player.Pause();
        Player.Source = null;
        Player.Dispose();
    }
}

/// <summary>
/// YouTube's own embedded player, where YouTube can be reached (youtube.ts;
/// Android's <c>YouTubeBackend</c>): its media addresses are bound to
/// whoever resolved them, so nobody can be handed one to play, and somebody
/// who can reach YouTube plays it through YouTube's player, driven through
/// its IFrame API.
///
/// The page is served as if from the deployment's own address (a request
/// for it is answered from here, never sent), so the player is embedded
/// from the same origin the web client embeds it from, and YouTube sees one
/// site asking whichever client is in front of it. In a WebView2 with its
/// own data folder, autoplay allowed (the asking is the show's, not a press
/// here), and, in a muted launch, every sound off.
/// </summary>
internal sealed class YouTubeBackend : IWatchBackend
{
    /// <summary>Long enough for a slow connection; short enough that a blocked one is not a long wait at a black tile (youtube.ts <c>LOAD_MS</c>).</summary>
    public const int LoadMs = 6000;
    private const int Ended = 0, PlayingState = 1, Buffering = 3;
    public const string Api = "https://www.youtube.com/iframe_api";

    public readonly WebView2 Web = new() { DefaultBackgroundColor = Colors.Black };
    private readonly WatchPlayer _owner;
    private readonly string _page;
    private readonly string _html;
    private bool _ready;
    private double _reported;
    private long _reportedAt;
    private int _state = -1;
    private bool _gone;

    public YouTubeBackend(string id, string baseUrl, WatchPlayer owner, string? api, bool silent)
    {
        _owner = owner;
        var origin = Regex.Match(baseUrl, @"^([a-zA-Z][a-zA-Z0-9+.-]*://[^/?#]+)").Groups[1].Value;
        _page = origin + "/__tomoshibi/watch-youtube";
        _html = Page(id, origin, api ?? Api);
        _ = Start(silent);
    }

    private static Task<CoreWebView2Environment>? _environment;

    /// <summary>
    /// The one environment the app's web views share: made once, with its
    /// own folder under the app's (or a test's), autoplay without a press,
    /// and in a muted launch every sound off, which only an environment's
    /// arguments can say.
    /// </summary>
    private static Task<CoreWebView2Environment> Environment(bool silent)
    {
        return _environment ??= Make();

        async Task<CoreWebView2Environment> Make()
        {
            var folder = Path.Combine(System.Environment.GetFolderPath(System.Environment.SpecialFolder.LocalApplicationData), silent ? "Tomoshibi-tests" : "Tomoshibi", "WebView2");
            var arguments = "--autoplay-policy=no-user-gesture-required" + (silent ? " --mute-audio" : "");
            return await CoreWebView2Environment.CreateWithOptionsAsync(null, folder, new CoreWebView2EnvironmentOptions { AdditionalBrowserArguments = arguments });
        }
    }

    private async Task Start(bool silent)
    {
        try
        {
            await Web.EnsureCoreWebView2Async(await Environment(silent));
        }
        catch (Exception fault)
        {
            // No WebView2 on this machine: as if YouTube could not be reached.
            Log.Line($"watch: no web view for youtube: {fault.Message}");
            if (!_gone) _owner.YoutubeUnreachable(this);
            return;
        }
        if (_gone) return;
        var core = Web.CoreWebView2;
        core.Settings.AreDefaultContextMenusEnabled = false;
        core.Settings.IsStatusBarEnabled = false;
        core.Settings.IsZoomControlEnabled = false;
        core.Settings.AreBrowserAcceleratorKeysEnabled = false;
        core.AddWebResourceRequestedFilter(_page, CoreWebView2WebResourceContext.Document);
        core.WebResourceRequested += (_, e) =>
        {
            if (e.Request.Uri != _page) return;
            var stream = new MemoryStream(System.Text.Encoding.UTF8.GetBytes(_html)).AsRandomAccessStream();
            e.Response = core.Environment.CreateWebResourceResponse(stream, 200, "OK", "Content-Type: text/html; charset=utf-8");
        };
        // The page and the player YouTube puts in it; nothing else is opened
        // in the view: a press on YouTube's name in the frame would take the
        // stage to YouTube's site.
        core.NavigationStarting += (_, e) =>
        {
            if (e.Uri != _page) e.Cancel = true;
        };
        core.NewWindowRequested += (_, e) => e.Handled = true;
        core.WebMessageReceived += (_, e) => Heard(e.TryGetWebMessageAsString());
        core.Navigate(_page);
    }

    public double Time
    {
        get
        {
            // The page says where it is ten times a second; between two
            // words a playing player has moved on by however long it has been.
            if (_state != PlayingState) return _reported;
            return _reported + (WatchWords.Now() - _reportedAt) / 1000.0;
        }
    }

    public bool Ready => _ready;
    public bool Playing => _state is PlayingState or Buffering;
    public double Rate => 1;
    public bool Nudges => false;
    public bool Youtube => true;
    public bool Flowing => _state == PlayingState;

    public void Play() => Run("watch.play()");
    public void Pause() => Run("watch.pause()");
    public void Seek(double to) => Run($"watch.seek({Math.Max(0, to).ToString(CultureInfo.InvariantCulture)})");
    public void SetRate(double rate) { }
    public void SetVolume(double level, bool muted) => Run($"watch.volume({level.ToString(CultureInfo.InvariantCulture)}, {(muted ? "true" : "false")})");
    public void SetTappable(bool on) => Run($"watch.tappable({(on ? "true" : "false")})");

    public void Stop()
    {
        if (_gone) return;
        Run("watch.destroy()");
        _gone = true;
        try
        {
            Web.Close();
        }
        catch (Exception)
        {
        }
    }

    private void Run(string script)
    {
        if (_gone || Web.CoreWebView2 == null) return;
        _ = Web.CoreWebView2.ExecuteScriptAsync("window.watch && " + script);
    }

    private void Heard(string body)
    {
        if (_gone) return;
        JsonElement message;
        try
        {
            message = JsonDocument.Parse(body).RootElement;
        }
        catch (Exception)
        {
            return;
        }
        if (message.ValueKind != JsonValueKind.Object || !message.TryGetProperty("t", out var kind)) return;
        switch (kind.GetString())
        {
            case "ready":
                _ready = true;
                _owner.BackendPlayable(this);
                break;
            case "tick":
            case "state":
                if (message.TryGetProperty("time", out var time) && time.TryGetDouble(out var at))
                {
                    _reported = at;
                    _reportedAt = WatchWords.Now();
                }
                var was = _state;
                if (message.TryGetProperty("state", out var state) && state.TryGetInt32(out var now)) _state = now;
                if (kind.GetString() == "state" || _state != was)
                {
                    if (_state == PlayingState) _owner.BackendBegan(this);
                    if (_state == Ended && was != Ended) _owner.BackendEnded(this);
                }
                break;
            case "error":
                _owner.YoutubeRefused(this, message.TryGetProperty("code", out var code) && code.TryGetInt32(out var number) ? number : 0);
                break;
            case "unreachable":
                _owner.YoutubeUnreachable(this);
                break;
        }
    }

    /// <summary>Every value as a JSON string, so nothing in one can end the script.</summary>
    private static string Quoted(string text) => JsonSerializer.Serialize(text);

    /// <summary>The page the player is put in: the other apps' own, its messages handed to WebView2's bridge.</summary>
    private static string Page(string id, string origin, string api) => $$"""
        <!doctype html><html><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width,initial-scale=1">
        <style>
        html,body{margin:0;width:100%;height:100%;background:#000;overflow:hidden}
        #player,iframe,video{position:absolute;inset:0;width:100%;height:100%;border:0}
        body.locked iframe,body.locked video{pointer-events:none}
        </style></head><body class="locked"><div id="player"></div><script>
        (() => {
          const say = (message) => window.chrome.webview.postMessage(JSON.stringify(message));
          let player, ready = false, gaveUp = false, sound = null;
          const hear = () => {
            if (!player || !ready || !sound) return;
            player.setVolume(Math.round(sound[0] * 100));
            sound[1] ? player.mute() : player.unMute();
          };
          // The API's script either arrives or it does not; from where it
          // does not, the connection usually hangs rather than fails.
          const fail = () => { if (!gaveUp && !ready) { gaveUp = true; say({ t: "unreachable" }); } };
          const timer = setTimeout(fail, {{LoadMs}});
          window.onYouTubeIframeAPIReady = () => {
            clearTimeout(timer);
            if (gaveUp) return;
            player = new YT.Player("player", {
              videoId: {{Quoted(id)}}, width: "100%", height: "100%",
              playerVars: { controls: 0, disablekb: 1, playsinline: 1, rel: 0, modestbranding: 1, iv_load_policy: 3, origin: {{Quoted(origin)}} },
              events: {
                onReady: () => { ready = true; hear(); say({ t: "ready" }); },
                onStateChange: (event) => say({ t: "state", state: event.data, time: player.getCurrentTime() }),
                onError: (event) => say({ t: "error", code: event.data }),
              },
            });
          };
          const script = document.createElement("script");
          script.src = {{Quoted(api)}};
          script.async = true;
          script.onerror = () => { clearTimeout(timer); fail(); };
          document.head.append(script);
          setInterval(() => { if (ready) say({ t: "tick", time: player.getCurrentTime(), state: player.getPlayerState() }); }, 100);
          window.watch = {
            play: () => player && player.playVideo(),
            pause: () => player && player.pauseVideo(),
            seek: (to) => player && player.seekTo(to, true),
            // Kept until the player is ready to be told: asked before then,
            // a volume would be lost and the video start at full sound.
            volume: (level, muted) => { sound = [level, muted]; hear(); },
            tappable: (on) => document.body.classList.toggle("locked", !on),
            destroy: () => { ready = false; player && player.destroy(); },
          };
        })();
        </script></body></html>
        """;
}
