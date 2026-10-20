using Microsoft.UI.Dispatching;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.UI.Room;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;

namespace Tomoshibi.Model.Watch;

/// <summary>What a ticket plays, as far as this app knows (watch-api.ts <c>readTicket</c>).</summary>
internal abstract record WatchInfo
{
    public sealed record Known(ResolvedVideo Resolved) : WatchInfo;

    /// <summary>Read, and playable only through a relay the server will not run for it: no retry plays it.</summary>
    public sealed record Unavailable : WatchInfo;

    /// <summary>Could not be read this time.</summary>
    public sealed record Failed : WatchInfo;

    public ResolvedVideo? Video => (this as Known)?.Resolved;
}

/// <summary>
/// The word a refusal of a link goes by (watch-api.ts; Android's
/// WatchWords.code), the same words the theatre says a refusal in, which
/// WatchWords.Refusal turns into a sentence. Kept beside the engine because
/// the shared rules know nothing of the core's types.
/// </summary>
internal static class WatchCodes
{
    public static string Code(UnresolvedReason reason) => reason switch
    {
        UnresolvedReason.NotAVideo => "not_a_video",
        UnresolvedReason.Unavailable => "unavailable",
        UnresolvedReason.MembersOnly => "members_only",
        UnresolvedReason.CopyProtected => "copy_protected",
        UnresolvedReason.RegionLocked => "region_locked",
        UnresolvedReason.NotFound => "not_found",
        UnresolvedReason.NotAllowed => "not_allowed",
        UnresolvedReason.EmbyNotAllowed => "emby_not_allowed",
        _ => "failed",
    };
}

/// <summary>
/// One call's show, as viewer and as holder (Android's
/// feature/watch/WatchEngine.kt; WatchFeature.swift <c>WatchEngine</c>;
/// web/src/live/watch.ts, WatchScreen.tsx, WatchPanel.tsx, WatchTab.tsx):
/// the protocol crate's <c>WatchTheatre</c> wired to the call as its module
/// header says, the videos' descriptions asked of the deployment, and the
/// player for whatever is on.
///
/// Made once per call and kept in the call's parts (<see cref="Of"/>),
/// because the room's views are made and remade, and a show made in a view
/// would say hello to the room every time and leave a timer running. UI
/// thread only, as the call's model is; everything it starts ends with the
/// call.
/// </summary>
internal sealed class WatchEngine : Observable, IBarOffer
{
    public const string Part = "watch";
    /// <summary>The data topic (watch.ts <c>TOPIC</c>).</summary>
    public const string Topic = "watch";
    /// <summary>The longest link a show reads, in UTF-16 units, as the web's field allows.</summary>
    public const int LinkLength = 2000;

    public readonly CallModel Call;
    public Deployment Deployment => Call.Deployment;
    public bool Sealed => Call.Sealed;
    private readonly WatchTheatre _theatre;
    private readonly DispatcherQueueTimer? _beat;

    /// <summary>The show as this participant knows it, and when it was heard: what the timeline is reckoned from.</summary>
    public WatchShow? Show { get; private set; }
    public long HeardAtMs { get; private set; }

    /// <summary>Whether there is a show at all, apart from <see cref="Show"/>, so the room's arrangement is not redrawn every time the holder says where it is.</summary>
    public bool OnStage { get; private set; }

    /// <summary>The show runs here.</summary>
    public bool Mine { get; private set; }

    /// <summary>Whether this person may read links and start a show: signed in, given watching together, on a deployment with a gateway (watch-api.ts <c>watchReady</c>). Asked once.</summary>
    public bool Ready { get; private set; }

    /// <summary>What each ticket plays, asked of the server once and kept for the call.</summary>
    public IReadOnlyDictionary<string, WatchInfo> Infos => _infos;
    private readonly Dictionary<string, WatchInfo> _infos = new();

    /// <summary>The media servers this person may browse, asked when the panel opens.</summary>
    public IReadOnlyList<MediaServer> Servers { get; private set; } = Array.Empty<MediaServer>();

    /// <summary>The player for what is on now, one per video as the web keys its player by the video's key.</summary>
    public WatchPlayer? Player { get; private set; }

    private readonly Dictionary<string, List<TaskCompletionSource<WatchInfo>>> _asking = new();
    private Task _sending = Task.CompletedTask;
    private bool _reconnecting;
    private bool _over;

    private WatchEngine(CallModel call, bool live = true)
    {
        Call = call;
        _theatre = new WatchTheatre(call.LocalIdentity, call.LocalName, call.Sealed);
        // The holder says the show closed before the call goes, as the web
        // does when the room unmounts; said after, it never arrived, and
        // everybody else kept a frozen show until they noticed the holder
        // had left.
        call.BeforeLeaving[Part] = Leaving;
        // Not offered at all in a sealed call: nothing is listened to, said
        // or asked, and the server is not even asked whether this person
        // could start a show (Room.tsx: "an offer there led to a panel that
        // could only say so").
        if (!live || Sealed) return;
        call.Listen(Hear);
        var now = Clock();
        // The people already here were announced during the join, before
        // anybody could listen, so the roster is read rather than heard.
        Handle(_theatre.Roster(call.Identities.ToArray(), now));
        Handle(_theatre.Joined(now));
        // Four times a second: the keeper's step (WatchScreen.tsx
        // `KEEP_MS`), and the theatre's tick, which it wants at least once a
        // second.
        _beat = DispatcherQueue.GetForCurrentThread().CreateTimer();
        _beat.Interval = TimeSpan.FromMilliseconds(WatchPlayer.KeepMs);
        _beat.Tick += (_, _) => Beat();
        _beat.Start();
        _ = AskReady();
    }

    /// <summary>The call's show, made the first time anybody asks for it, with what it offers the bar beside it.</summary>
    public static WatchEngine Of(CallModel call)
    {
        if (call.Parts.TryGetValue(Part, out var made) && made is WatchEngine engine) return engine;
        engine = new WatchEngine(call);
        call.Parts[Part] = engine;
        call.Parts[IBarOffer.Watch] = engine;
        return engine;
    }

    private static ulong Clock() => (ulong)WatchWords.Now();

    private async Task AskReady()
    {
        bool answer;
        try
        {
            answer = await Deployment.WatchReady();
        }
        catch (Exception)
        {
            answer = false;
        }
        if (_over) return;
        Ready = answer;
        Notify();
    }

    /// <summary>
    /// What the control bar offers (WatchFeature.swift <c>offered</c>,
    /// <c>playing</c>; Room.tsx ~295): the button to whoever can start a show,
    /// and to everybody once there is one; never in a sealed call; and a dot
    /// on it while the show plays.
    /// </summary>
    public bool Offered => !Sealed && (Ready || OnStage);
    public bool Playing => Show?.Playing == true;

    // The call

    private void Hear(CallEvent e)
    {
        if (_over) return;
        var now = Clock();
        switch (e)
        {
            case CallEvent.ParticipantJoined joined:
                Handle(_theatre.ParticipantConnected(joined.Participant.Identity, now));
                break;
            case CallEvent.ParticipantLeft left:
                Handle(_theatre.ParticipantDisconnected(left.Identity, now));
                break;
            case CallEvent.StateChanged changed:
                if (changed.State == CallState.Reconnecting) _reconnecting = true;
                else if (changed.State == CallState.Connected && _reconnecting)
                {
                    // A reconnect replaces the roster without saying who left.
                    _reconnecting = false;
                    Handle(_theatre.Roster(Call.Identities.ToArray(), now));
                }
                else if (changed.State == CallState.Disconnected) End();
                break;
            case CallEvent.Data data:
                // Only from a participant: the server says nothing on this
                // topic, and anything arriving without a sender is not a
                // show's word.
                if (data.Identity is not { } sender || data.Topic != Topic) return;
                Handle(_theatre.OnMessage(sender, Call.PersonOf(sender)?.Name ?? "", data.Payload, now));
                break;
            case CallEvent.Ended:
                End();
                break;
        }
    }

    /// <summary>
    /// The call is over: the player stops at once rather than when the
    /// call's model is let go, which would leave a film playing after the
    /// person left the room. For good: every late answer looks at
    /// <see cref="_over"/> first (Android's note: a description fetched after
    /// this read the show again and started the film on a call already left).
    /// </summary>
    private void End()
    {
        if (_over) return;
        _over = true;
        _beat?.Stop();
        Player?.Stop();
        Player = null;
        _theatre.Close(Clock());
        Show = null;
        OnStage = false;
        Notify();
    }

    private void Beat()
    {
        if (_over) return;
        if (Call.Over.IsCancellationRequested)
        {
            End();
            return;
        }
        var now = Clock();
        Handle(_theatre.Tick(now));
        Refresh(now);
        // The clock read again: the tick or the refresh may have just built
        // a player, whose source began after `now` (WatchStall).
        if (Player is { } player && Show is { } show) player.Keep(WatchWords.Now(), show, HeardAtMs);
    }

    /// <summary>Reads the show again and follows it: a new video gets a player of its own, and a show that ended takes its player with it.</summary>
    internal void Refresh(ulong now)
    {
        if (_over) return;
        var snapshot = _theatre.State(now);
        var changed = false;
        var heard = (long)snapshot.HeardAtMs;
        if (!Equals(snapshot.Show, Show) || heard != HeardAtMs)
        {
            Show = snapshot.Show;
            HeardAtMs = heard;
            changed = true;
        }
        if (snapshot.Mine != Mine)
        {
            Mine = snapshot.Mine;
            changed = true;
        }
        if ((Show != null) != OnStage)
        {
            OnStage = Show != null;
            changed = true;
        }
        // A video that stopped being playable here (AskAgain learned the
        // server will not relay it) takes its player with it too, and the
        // stage says so in its place.
        var video = Show?.Now;
        var playable = video != null && _infos.TryGetValue(video.Ticket, out var known) ? known.Video : null;
        if (Player is { } current && (current.Video.Key != video?.Key || playable == null))
        {
            current.Stop();
            Player = null;
            changed = true;
        }
        if (video != null && Player == null && Info(video.Ticket)?.Video is { } resolved)
        {
            Player = new WatchPlayer(video, resolved, this, Deployment.BaseUrl(), TomoshibiProtocolMethods.Mainland(Call.Region));
            changed = true;
        }
        if (changed) Notify();
    }

    /// <summary>Something about the player changed that the stage draws; the stage listens to the engine alone.</summary>
    internal void PlayerChanged() => Notify();

    // What the theatre says

    private void Handle(WatchOutput output, bool answering = true)
    {
        // Nothing the theatre says once the call is over is said: its words
        // would go into a call already left, and its asks start the fetches
        // that brought the show back.
        if (_over) return;
        foreach (var packet in output.Packets) Send(packet);
        foreach (var why in output.Refusals) Say(WatchWords.Refusal(why));
        if (answering)
        {
            foreach (var ask in output.Asks) _ = AnswerAndHandle(ask);
        }
        if (output.Changed) Refresh(Clock());
    }

    private async Task AnswerAndHandle(WatchAsk ask) => Handle(await Answer(ask));

    /// <summary>
    /// Published in the order the theatre said them: one at a time, each
    /// after the last has gone, since a state overtaken by the word that the
    /// show closed would have everybody believe in it again.
    /// </summary>
    private void Send(WatchPacket packet)
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
                Log.Line($"watch: a word did not go: {fault.Message}");
            }
        }
    }

    /// <summary>Finds out what the theatre asked and gives it the answer.</summary>
    private async Task<WatchOutput> Answer(WatchAsk ask)
    {
        switch (ask)
        {
            case WatchAsk.Resolve resolve:
            {
                WatchAnswer answer;
                try
                {
                    answer = await Deployment.ResolveVideo(resolve.Text, resolve.ForOther) switch
                    {
                        ResolvedLink.Playable found => Found(found.Video),
                        ResolvedLink.NotPlayable not => new WatchAnswer.Refused(WatchCodes.Code(not.Reason)),
                        _ => new WatchAnswer.Refused("failed"),
                    };
                }
                catch (Exception)
                {
                    // What the web's resolveVideo makes of anything thrown.
                    answer = new WatchAnswer.Refused("failed");
                }
                return _theatre.Answer(resolve.Request, answer, Clock());
            }
            case WatchAsk.Read read:
            {
                var found = await ReadTicket(read.Ticket);
                WatchAnswer answer = found switch
                {
                    WatchInfo.Known known => new WatchAnswer.Found(known.Resolved.Ticket, known.Resolved.Duration, known.Resolved.Live, known.Resolved.Grantor),
                    WatchInfo.Unavailable => new WatchAnswer.Refused("unavailable"),
                    _ => new WatchAnswer.Refused("failed"),
                };
                return _theatre.Answer(read.Request, answer, Clock());
            }
        }
        throw new InvalidOperationException("an ask this app does not know");
    }

    private WatchAnswer Found(ResolvedVideo video)
    {
        Remember(video);
        return new WatchAnswer.Found(video.Ticket, video.Duration, video.Live, video.Grantor);
    }

    private async Task<WatchInfo> ReadInfo(string ticket)
    {
        try
        {
            return await Deployment.VideoInfo(ticket) switch
            {
                TicketVideo.Known known => new WatchInfo.Known(known.Video),
                TicketVideo.Unavailable => new WatchInfo.Unavailable(),
                _ => new WatchInfo.Failed(),
            };
        }
        catch (Exception)
        {
            return new WatchInfo.Failed();
        }
    }

    /// <summary>Said in the call's corner, as the web says a refusal (Room.tsx <c>hearShows</c>), wherever the panel is.</summary>
    public void Say(string text)
    {
        if (!_over) Call.ActionFailed(text);
    }

    // What each ticket plays (watch-api.ts)

    /// <summary>What a ticket plays, if this app knows yet; asks the server once if not, and the answer redraws whoever asked.</summary>
    public WatchInfo? Info(string ticket)
    {
        if (_infos.TryGetValue(ticket, out var known)) return known;
        if (!_asking.ContainsKey(ticket) && !_over)
        {
            _asking[ticket] = new List<TaskCompletionSource<WatchInfo>>();
            _ = Fetch(ticket);
        }
        return null;
    }

    /// <summary>The same question, waited for (watch-api.ts <c>readTicket</c>).</summary>
    private Task<WatchInfo> ReadTicket(string ticket)
    {
        if (Info(ticket) is { } known) return Task.FromResult(known);
        var waiting = new TaskCompletionSource<WatchInfo>();
        if (!_asking.TryGetValue(ticket, out var list)) _asking[ticket] = list = new List<TaskCompletionSource<WatchInfo>>();
        list.Add(waiting);
        return waiting.Task;
    }

    /// <summary>Whoever waits is always answered, even after the call is over, since a wait left unanswered is a task that never ends.</summary>
    private async Task Fetch(string ticket)
    {
        var found = await ReadInfo(ticket);
        _infos[ticket] = found;
        if (_asking.Remove(ticket, out var waiting))
        {
            foreach (var one in waiting) one.TrySetResult(found);
        }
        Notify();
        Refresh(Clock());
    }

    /// <summary>
    /// What a ticket plays, asked again, for a player whose relay stopped
    /// partway through a video that plays nowhere else (watch-api.ts
    /// <c>askAgain</c>). Only "the server will not relay it" replaces what
    /// was known.
    /// </summary>
    public async Task<WatchInfo> AskAgain(string ticket)
    {
        var found = await ReadInfo(ticket);
        if (found is WatchInfo.Unavailable)
        {
            _infos[ticket] = found;
            Notify();
            Refresh(Clock());
        }
        return found;
    }

    /// <summary>What this person read themselves, so it is not asked for again.</summary>
    private void Remember(ResolvedVideo video)
    {
        _infos[video.Ticket] = new WatchInfo.Known(video);
        Notify();
    }

    // What the person does

    /// <summary>
    /// Adds what a link plays to the show; true if it was added, or sent to
    /// be (WatchTab.tsx <c>addVideo</c>). Read here by anybody who may read
    /// links, and the show is handed only the ticket; started here with it
    /// when nobody runs one. Somebody who may not hands the link itself to
    /// whoever runs the show, who reads it on their behalf. Read here rather
    /// than by the holder because of whose choice a video is: a media
    /// server's film is relayed only for the person given that who chose it.
    /// </summary>
    public async Task<bool> Add(string pasted)
    {
        var text = pasted.Trim();
        if (text.Length == 0 || Sealed || _over) return false;
        var now = Clock();

        if (!Ready)
        {
            var output = _theatre.AddLink(text, now);
            Handle(output);
            return output.Accepted;
        }

        Refresh(now);
        if (Show == null)
        {
            var started = _theatre.Start(now);
            Handle(started);
            if (!started.Accepted) return false;
            return await Own(_theatre.AddLink(text, Clock()));
        }
        if (Mine) return await Own(_theatre.AddLink(text, now));

        ResolvedLink found;
        try
        {
            found = await Deployment.ResolveVideo(WatchWords.Cut(text, LinkLength), false);
        }
        catch (Exception)
        {
            Say(WatchWords.Refusal("failed"));
            return false;
        }
        // Left, or ended, while the link was being read: nothing more.
        if (_over) return false;
        switch (found)
        {
            case ResolvedLink.NotPlayable not:
                Say(WatchWords.Refusal(WatchCodes.Code(not.Reason)));
                return false;
            case ResolvedLink.Playable playable:
                Remember(playable.Video);
                // The link beside its ticket, for a holder whose page predates
                // tickets; never for a media server's film, which such a
                // holder would read with its own session and so as its own
                // choice.
                var link = playable.Video.Play.Kind == ResolvedPlayKind.Relay ? null : text;
                return await Own(_theatre.AddTicket(playable.Video.Ticket, link, Clock()));
        }
        return false;
    }

    /// <summary>An output whose asks are this person's own adding: answered before saying whether it worked, so the field empties only for a video that went in.</summary>
    private async Task<bool> Own(WatchOutput output)
    {
        Handle(output, answering: false);
        var added = output.Accepted && output.Refusals.Length == 0;
        foreach (var ask in output.Asks)
        {
            var answered = await Answer(ask);
            if (_over) return false;
            Handle(answered);
            if (answered.Refusals.Length > 0) added = false;
        }
        return added && !_over;
    }

    /// <summary>Play or pause for everybody, felt here at once: the player is told too (WatchScreen.tsx <c>toggle</c>).</summary>
    public void Toggle()
    {
        if (Show is not { } show) return;
        if (show.Playing)
        {
            Player?.PauseNow();
            Handle(_theatre.Pause(Clock()));
        }
        else
        {
            Player?.PlayNow();
            Handle(_theatre.Play(Clock()));
        }
    }

    /// <summary>Moves everybody to <paramref name="to"/>, and this player at once (<c>seekTo</c>).</summary>
    public void Seek(double to)
    {
        Player?.SeekNow(to);
        Handle(_theatre.Seek(to, Clock()));
    }

    /// <summary>Past the video named, and only that one: two presses do not skip two.</summary>
    public void Skip(string key) => Handle(_theatre.Skip(key, Clock()));

    public void Remove(string key) => Handle(_theatre.Remove(key, Clock()));

    /// <summary>The call is about to be left: a holder closes the show and waits for that word to go out.</summary>
    private async Task Leaving()
    {
        if (!Mine || _over) return;
        Handle(_theatre.Close(Clock()));
        try
        {
            await _sending;
        }
        catch (Exception)
        {
        }
    }

    /// <summary>Stop running the show here, saying so last of all.</summary>
    public void Stop() => Handle(_theatre.Close(Clock()));

    public void PlayerReady(string key) => Handle(_theatre.PlayerReady(key, Clock()));

    public void PlayerEnded(string key) => Handle(_theatre.PlayerEnded(key, Clock()));

    /// <summary>
    /// The media servers, asked when the panel opens and only of somebody who
    /// could start a show: browsing is for choosing, and choosing is theirs
    /// (WatchPanel.tsx). The web reads every refusal here as none.
    /// </summary>
    public async Task LoadServers()
    {
        if (!Ready || Sealed || _over) return;
        MediaServer[] found;
        try
        {
            found = await Deployment.MediaServers();
        }
        catch (Exception)
        {
            found = Array.Empty<MediaServer>();
        }
        if (_over || found.SequenceEqual(Servers)) return;
        Servers = found;
        Notify();
    }

    /// <summary>Whether the call's launch is muted: the video plays as everything else, at nought, so a test hears nothing.</summary>
    public bool Muted => Call.Launch.Mute;
}
