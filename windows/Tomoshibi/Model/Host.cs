using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.Model;

/// <summary>
/// The sound panel's record for this app, kept where the web keeps it and
/// under the web's name (hearing.ts; Android's model/host/Hearing.kt). One
/// for the whole app rather than one per call, as the web's is one per page:
/// a call that is moved, or comes back after a drop, is joined again as the
/// same identity, and its settings should be waiting for it. Nowhere in a
/// rehearsal: a test's mute is written against a proven identity like
/// anybody's, and would silence that person in the owner's next real call.
/// </summary>
internal sealed class Hearing
{
    private readonly IDefaults? _defaults;
    private readonly HearingBook _book;

    public Hearing(IDefaults? defaults)
    {
        _defaults = defaults;
        _book = new HearingBook(defaults?.String(HearingBook.StorageKey));
    }

    /// <summary>The app's one record, set when the app knows whether this is a rehearsal.</summary>
    public static Hearing Shared { get; set; } = new(null);

    /// <summary>Said whenever a setting changes, so that whatever drew one draws it again.</summary>
    public event Action? Changed;

    public HearingSetting Setting(string identity, Sound sound) => _book.Setting(Filing(identity), sound);

    public void Set(string identity, Sound sound, HearingSetting setting)
    {
        if (_book.Set(Filing(identity), sound, setting))
        {
            if (_book.Stored is { } stored) _defaults?.Set(HearingBook.StorageKey, stored);
            else _defaults?.Remove(HearingBook.StorageKey);
        }
        Changed?.Invoke();
    }

    /// <summary>
    /// Filed under the signature for a proven identity, which is the same on
    /// every visit, and under the identity itself otherwise, for the call
    /// only (hearing.ts <c>filing</c>).
    /// </summary>
    public static HearingFiling Filing(string identity)
    {
        if (TomoshibiProtocolMethods.SignatureOf(identity) is { Proven: true } signature) return new HearingFiling(signature.Trip, Keeps: true);
        return new HearingFiling(identity, Keeps: false);
    }
}

/// <summary>
/// Running a room and hearing it: the host's controls, the door,
/// invitations, every person's volume (web/src/live/host.ts, knock.ts,
/// hearing.ts; components/room/HostPanel.tsx, AtTheDoor.tsx,
/// SoundPanel.tsx; Android's model/host/HostFeature.kt, decision for
/// decision).
///
/// One per call, kept in the call's parts: the room is drawn again whenever
/// the language or the width changes, and a feature made with each drawing
/// would listen to the call once more every time. Nothing happens until
/// <see cref="Run"/>, which the room calls while the call is on screen.
///
/// UI thread only.
/// </summary>
internal sealed class HostFeature : Observable
{
    /// <summary>The name the door's notice is raised and drawn under.</summary>
    public const string Door = "door";
    private const string Part = "host";
    /// <summary>AtTheDoor.tsx <c>LOOK_EVERY</c>.</summary>
    public static readonly TimeSpan LookEvery = TimeSpan.FromSeconds(4);

    public readonly CallModel Call;

    private HostFeature(CallModel call) => Call = call;

    /// <summary>The call's feature, made the first time anybody asks.</summary>
    public static HostFeature Of(CallModel call)
    {
        if (call.Parts.TryGetValue(Part, out var part)) return (HostFeature)part;
        var made = new HostFeature(call);
        call.Parts[Part] = made;
        return made;
    }

    /// <summary>What the server last said about this participant and this room (host.ts <c>useStanding</c>): nothing until it has said.</summary>
    public HostStanding Standing { get; private set; } = new(false, false);

    private IReadOnlyList<Knocker> _waiting = Array.Empty<Knocker>();
    /// <summary>Who is knocking, for as long as this participant can let them in.</summary>
    public IReadOnlyList<Knocker> Waiting
    {
        get => _waiting;
        private set
        {
            _waiting = value;
            SayWhoIsWaiting();
            Notify();
        }
    }

    /// <summary>The knock being answered, so its buttons do nothing twice.</summary>
    public string? Answering { get; private set; }

    /// <summary>The link made by the last press of "Invite somebody", until it is revoked.</summary>
    public string? Link { get; private set; }
    public bool Copied { get; private set; }

    /// <summary>The one action under way, named by what it acts on (HostPanel.tsx <c>busy</c>): an identity, or "invite", "revoke", "move", "close".</summary>
    public string? Busy { get; private set; }

    /// <summary>The relays a meeting can be moved to, once asked for.</summary>
    public IReadOnlyList<Relay>? Relays { get; private set; }

    /// <summary>Asked once rather than done at once: ending a meeting for eleven people cannot be undone (HostPanel.tsx <c>ending</c>).</summary>
    public bool Ending { get; set; }

    /// <summary>Whether the list of relays to move to is open (HostPanel.tsx <c>Elsewhere</c>'s <c>open</c>).</summary>
    public bool ChoosingRelay { get; set; }

    private bool _panelOpen;
    /// <summary>
    /// Whether the host panel is on screen: the door's notice stands down
    /// while it is, because the panel lists the same people. Closing it puts
    /// both questions away; kept for the whole call, the panel opened again
    /// on "Close the room" already armed, one press from ending the meeting
    /// for everybody.
    /// </summary>
    public bool PanelOpen
    {
        get => _panelOpen;
        set
        {
            _panelOpen = value;
            if (!value)
            {
                Ending = false;
                ChoosingRelay = false;
            }
            SayWhoIsWaiting();
            Notify();
        }
    }

    /// <summary>How many people the door's notice last said were waiting; nought while it is not up.</summary>
    private int _doorSaid;

    /// <summary>A move older than this, by the call's clock, is no longer shown, whatever became of it (App.tsx: twenty seconds).</summary>
    private long? _movingGaveUp;

    private bool _started;

    /// <summary>The sounds this feature asked to stop at the server, so it only ever lets flow again what it stopped itself.</summary>
    private readonly HashSet<string> _stopped = new();

    /// <summary>Whether this room is held under a scope, where every member runs it and there is nobody to hand it to (HostPanel.tsx <c>scoped</c>).</summary>
    public bool Scoped => TomoshibiProtocolMethods.ScopeOf(Call.Room).Length > 0;

    /// <summary>Where a move is taking the meeting, while that is worth showing.</summary>
    public string? MovingTo
    {
        get
        {
            if (Call.MovingNow is not { } moving) return null;
            if (_movingGaveUp is { } gaveUp && gaveUp >= moving.At) return null;
            return moving.Relay;
        }
    }

    // Running

    /// <summary>
    /// Everything this feature does while the call is on screen: listening to
    /// the call, asking who runs the room, watching the door every four
    /// seconds while this participant may open it. Returns when the call is
    /// over.
    /// </summary>
    public async Task Run()
    {
        if (_started) return;
        _started = true;
        Call.Listen(Heard);
        Hearing.Shared.Changed += ApplyHearing;
        ApplyHearing();
        await RefreshStanding();
        while (!Call.Over.IsCancellationRequested)
        {
            if (Standing.Yours) await LookAtTheDoor();
            else if (Waiting.Count > 0) Waiting = Array.Empty<Knocker>();
            try
            {
                await Task.Delay(LookEvery, Call.Over);
            }
            catch (OperationCanceledException)
            {
                break;
            }
        }
        Hearing.Shared.Changed -= ApplyHearing;
    }

    private void Heard(CallEvent e)
    {
        switch (e)
        {
            // Somebody leaving is the ordinary reason a room changes hands,
            // and a connection that came back may have missed whatever
            // happened while it was away (host.ts `useStanding`).
            case CallEvent.ParticipantLeft:
            case CallEvent.StateChanged:
                _ = RefreshStanding();
                break;
            // A handover, said by the server and only by the server: a
            // participant's packet always arrives with its sender, and this
            // one with none (host.ts `told`).
            case CallEvent.Data data when data.Topic == "host" && data.Identity == null:
                _ = RefreshStanding();
                break;
            // A sound that has just arrived was given a player at full volume
            // and is flowing; whatever was decided about it is put back
            // (Audible.tsx `HEARING_EVENTS`).
            case CallEvent.ParticipantJoined:
            case CallEvent.TrackPublished:
            case CallEvent.TrackSubscribed:
                ApplyHearing();
                break;
            case CallEvent.TrackUnpublished unpublished:
                _stopped.Remove(unpublished.Sid);
                break;
            // The moving overlay is shown for twenty seconds at most
            // (App.tsx): a move that never ends the call would otherwise
            // leave "you will be back in a moment" over the controls.
            case CallEvent.Data data when data.Identity == null && data.Topic is "moving" or "placing":
                var at = Call.MovingNow?.At;
                _ = Task.Delay(20_000, Call.Over).ContinueWith(_ =>
                {
                    if (at is { } when) MainWindow.Current?.DispatcherQueue.TryEnqueue(() => GiveUpMoving(when));
                }, TaskScheduler.Default);
                break;
        }
    }

    private string Bearer => Call.Core.Token();

    public async Task RefreshStanding()
    {
        HostStanding said;
        try
        {
            said = await Call.Deployment.HostStanding(Call.Room, Bearer);
        }
        catch (Exception)
        {
            // A round that did not answer says nothing about who runs it.
            return;
        }
        var became = said.Yours && !Standing.Yours;
        Standing = said;
        Call.Standing(said.Yours);
        SayWhoIsWaiting();
        if (!said.Yours)
        {
            if (Waiting.Count > 0) Waiting = Array.Empty<Knocker>();
            Link = null;
            Copied = false;
        }
        Notify();
        // Looked at once at once rather than at the next round, so a person
        // handed a room with somebody already outside sees them now.
        if (became) await LookAtTheDoor();
    }

    // The door

    private async Task LookAtTheDoor()
    {
        if (!Standing.Yours) return;
        Knocker[] said;
        try
        {
            said = await Call.Deployment.Knocking(Call.Room, Bearer);
        }
        catch (Exception)
        {
            // A round that did not answer is not an empty door (AtTheDoor.tsx).
            return;
        }
        if (!said.SequenceEqual(Waiting)) Waiting = said;
    }

    public async Task Answer(Knocker knocker, bool admit)
    {
        if (Answering != null) return;
        Answering = knocker.Id;
        Notify();
        try
        {
            await Call.Deployment.AnswerKnock(Call.Room, knocker.Id, Bearer, admit);
            // Taken off the list now rather than at the next round, so a
            // press does something the moment it is pressed.
            Waiting = Waiting.Where(k => k.Id != knocker.Id).ToList();
        }
        catch (Exception)
        {
            await LookAtTheDoor();
        }
        finally
        {
            Answering = null;
            Notify();
        }
    }

    // The host's actions

    /// <summary>One action at a time, and what went wrong said in words chosen for its code (HostPanel.tsx <c>act</c>).</summary>
    private async Task Act(string who, Func<Task> work)
    {
        if (Busy != null) return;
        Busy = who;
        Notify();
        try
        {
            await work();
        }
        catch (Exception fault)
        {
            Log.Line($"host: {who} refused: {Faults.Code(fault) ?? fault.Message}");
            Say(Explain(fault), danger: true);
        }
        finally
        {
            Busy = null;
            Notify();
        }
    }

    /// <summary>Makes a link anybody can join by, and puts it on the clipboard, since whoever pressed this is about to paste it somewhere.</summary>
    public Task Invite(Func<string, bool> copy) => Act("invite", async () =>
    {
        var made = await Call.Deployment.CreateInvite(Call.Room, Bearer);
        var address = Call.Deployment.InviteLink(made.Token, Call.Room);
        Link = address;
        Copied = copy(address);
    });

    public Task Revoke() => Act("revoke", async () =>
    {
        var gone = (int)await Call.Deployment.RevokeInvites(Call.Room, Bearer);
        Link = null;
        Copied = false;
        // Said, because "nothing happened" and "three links stopped working"
        // look the same otherwise.
        Say(gone == 1 ? T("One link stopped working.") : T("{count} links stopped working.", ("count", gone)));
    });

    /// <summary>Quietens somebody's microphone from the server, not by asking them.</summary>
    public Task Silence(Person person)
    {
        if (person.Track(MediaSource.Microphone) is not { } microphone) return Task.CompletedTask;
        return Act(person.Id, () => Call.Deployment.Silence(Call.Room, Bearer, person.Id, microphone.Sid));
    }

    public Task TurnOut(Person person) => Act(person.Id, () => Call.Deployment.TurnOut(Call.Room, Bearer, person.Id));

    public async Task HandOver(Person person)
    {
        await Act(person.Id, () => Call.Deployment.HandOver(Call.Room, Bearer, person.Id));
        // The server says so to the room, which asks again; asked here as
        // well, so the panel does not wait on a packet to stop offering
        // controls that now answer "not yours".
        await RefreshStanding();
    }

    public Task Move(Relay relay) => Act("move", () => Call.Deployment.MoveRoom(Call.Room, Bearer, relay.Name));

    public Task Dissolve() => Act("close", () => Call.Deployment.Dissolve(Call.Room, Bearer));

    /// <summary>The relays on offer, asked once, when the panel that offers them is first opened (HostPanel.tsx <c>Elsewhere</c>).</summary>
    public async Task LoadRelays()
    {
        if (Relays != null) return;
        try
        {
            Relays = (await Call.Deployment.Relays()).Relays;
        }
        catch (Exception)
        {
            Relays = Array.Empty<Relay>();
        }
        Notify();
    }

    /// <summary>
    /// Where the meeting can be put: neither a machine under maintenance nor,
    /// for anybody but an administrator, one held in reserve. The server
    /// refuses those regardless; this is about not showing a door that will
    /// not open.
    /// </summary>
    public IReadOnlyList<Relay> UsableRelays => (Relays ?? Array.Empty<Relay>()).Where(r => !r.Maintenance && (Standing.Admin || !r.AdminOnly)).ToList();

    /// <summary>Whether somebody joined without a passphrase, so that the room cannot be handed to them (HostPanel.tsx <c>HandOver</c>, by the signature's prefix).</summary>
    public static bool IsGuest(Person person) => TomoshibiProtocolMethods.SignatureOf(person.Id) is { Proven: false };

    public void RefuseHandOver() => Say(T("They joined without a passphrase, so they cannot run the room. Ask them to rejoin with one."), danger: true);

    // Hearing

    public static HearingSetting Setting(Person person, Sound sound) => Hearing.Shared.Setting(person.Id, sound);

    public void SetVolume(Person person, Sound sound, double volume) =>
        Hearing.Shared.Set(person.Id, sound, Hearing.Shared.Setting(person.Id, sound) with { Volume = volume });

    public void SetBlocked(Person person, Sound sound, bool blocked) =>
        Hearing.Shared.Set(person.Id, sound, Hearing.Shared.Setting(person.Id, sound) with { Blocked = blocked });

    /// <summary>
    /// Puts every setting back on the call: all of them, every time, rather
    /// than working out which one an event was about (Audible.tsx
    /// <c>apply</c>).
    ///
    /// A block is asked of the media server, so nothing is downloaded. The
    /// core can only ask that of a track it is subscribed to, and it
    /// subscribes to every sound the moment it is published and plays it at
    /// once; so a blocked sound is also turned to nothing here, which covers
    /// the moment between its first packets and the server hearing the
    /// block. Only a sound this stopped is ever let flow again, so a track
    /// something else held back stays held back. A screen's sound and music
    /// are the core's to switch, together with the lossless streams that
    /// may stand in for them, so those are told to it instead.
    /// </summary>
    public void ApplyHearing()
    {
        foreach (var person in Call.People)
        {
            foreach (var sound in Sounds.All)
            {
                if (TrackOf(person, sound) is not { } track) continue;
                var setting = Hearing.Shared.Setting(person.Id, sound);
                Call.SetVolume(track.Sid, setting.Blocked ? 0f : (float)setting.Volume);
                if (sound == Sound.Voice)
                {
                    if (setting.Blocked)
                    {
                        // Noted before it is asked, not after it succeeded:
                        // the media library switches the track off here
                        // before it tells the server.
                        _stopped.Add(track.Sid);
                        Call.Enable(track.Sid, false);
                    }
                    else if (_stopped.Remove(track.Sid))
                    {
                        Call.Enable(track.Sid, true);
                    }
                }
                else
                {
                    _ = Call.HearShared(person.Id, sound == Sound.Music, (float)setting.Volume, setting.Blocked);
                }
            }
        }
        Notify();
    }

    /// <summary>The track each sound is carried on (hearing.ts <c>SOURCE</c>).</summary>
    public static TrackInfo? TrackOf(Person person, Sound sound) => sound switch
    {
        Sound.Voice => person.Track(MediaSource.Microphone),
        Sound.Screen => person.Track(MediaSource.ScreenShareAudio),
        _ => person.MusicTrack,
    };

    // Notices

    /// <summary>What a press did, said in the corner with everything else the call says (notices.ts <c>actionFailed</c>, <c>actionDone</c>).</summary>
    public void Say(string text, bool danger = false)
    {
        if (danger) Call.ActionFailed(text);
        else Call.Announce(new CallToast(text, Lasts: NoticeTimes.AtMost));
    }

    /// <summary>
    /// Somebody waiting to be let in, said where the host will see it and
    /// taken back the moment the door is empty (notices.ts <c>atTheDoor</c>).
    /// This one carries the people outside under its words, the name somebody
    /// typed and the address they knocked from, so the decision can be made
    /// where it is read. It has no duration and no close button: somebody is
    /// outside until they are answered or they leave. It stands down while
    /// the panel is open, which lists the same people.
    /// </summary>
    private void SayWhoIsWaiting()
    {
        var count = PanelOpen || !Standing.Yours ? 0 : Waiting.Count;
        if (count == _doorSaid) return;
        _doorSaid = count;
        if (count == 0)
        {
            Call.Dismiss(Door);
            return;
        }
        Call.Announce(new CallToast(
            count == 1 ? T("Somebody is waiting to be let in") : T("{count} people are waiting to be let in", ("count", count)),
            Kind: CallToast.Kinds.Signal, Lasts: null, Key: Door, Closable: false, Attachment: Door));
    }

    /// <summary>The move that began then is no longer worth showing.</summary>
    public void GiveUpMoving(long at)
    {
        _movingGaveUp = at;
        Notify();
    }

    /// <summary>The server sends a code; this owns the sentence (HostPanel.tsx <c>explain</c>), each written out because each wants a different next move.</summary>
    public static string Explain(Exception error) => Faults.Code(error) switch
    {
        "media_unreachable" => T("That server did not answer. The meeting has not been moved."),
        "no_such_relay" => T("That server is no longer on this deployment."),
        "relay_not_allowed" => T("That server is reserved, and this meeting may not be put on it."),
        "no_such_person" => T("They have already left."),
        "not_proven" => T("They joined without a passphrase, so they cannot run the room. Ask them to rejoin with one."),
        "not_yours" => T("You no longer run this room."),
        "scoped_room" => T("Every member of this room's scope runs it, so there is nobody to hand it to."),
        "rate_limited" => T("Too many attempts. Try again in a moment."),
        _ => T("That could not be done. Try again."),
    };
}
