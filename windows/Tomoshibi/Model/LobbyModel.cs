using System.Globalization;
using System.Text.Json;
using System.Text.Json.Nodes;
using Tomoshibi.Basics;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.Model;

/// <summary>
/// What the screens before a call show beside their forms: the meetings this
/// account has arranged (Schedule.tsx), the rooms joined lately, and the
/// relays as measured from here (ServerPicker.tsx); Android's
/// model/LobbyModel.kt. Kept apart from the views because a narrow window's
/// home and a wide one's sidebar show the same things laid out differently,
/// and a list fetched twice is a list that disagrees with itself for a
/// moment. For the UI thread.
/// </summary>
internal sealed class LobbyModel : Observable
{
    public const string RecentsKey = "tomoshibi.recents";
    public const int RecentsKept = 24;

    private readonly AppModel _app;
    private readonly IDefaults _defaults;

    public LobbyModel(AppModel app, IDefaults defaults)
    {
        _app = app;
        _defaults = defaults;
        Recents = LoadRecents();
        // The lobby redraws with the app: its lists depend on who is signed
        // in and which relays there are.
        app.Changed += Notify;
    }

    // Arranged meetings

    public enum Status { Loading, Ready, Failed, SignedOut }

    public IReadOnlyList<Arrangement> Arrangements { get; private set; } = Array.Empty<Arrangement>();
    public Status State { get; private set; } = Status.Loading;
    private int _reads;

    /// <summary>The ones still ahead or under way, soonest first.</summary>
    public IReadOnlyList<Arrangement> Upcoming => Arrangements.Where(a => !a.Ended).ToList();

    public async Task RefreshArrangements()
    {
        if (_app.Deployment is not { } deployment || _app.Account == null) return;
        var mine = ++_reads;
        State = Status.Loading;
        Notify();
        try
        {
            var found = await deployment.Arrangements();
            if (mine != _reads) return;
            Arrangements = found;
            State = Status.Ready;
        }
        catch (ProtocolException.Refused fault)
        {
            if (mine != _reads) return;
            State = fault.code == "not_signed_in" ? Status.SignedOut : Status.Failed;
        }
        catch (Exception)
        {
            if (mine != _reads) return;
            State = Status.Failed;
        }
        Notify();
    }

    /// <summary>Arranges one and returns it, or the sentence why not.</summary>
    public async Task<(Arrangement? Made, string? Refusal)> Arrange(string room, DateTimeOffset at, string? relay)
    {
        if (_app.Deployment is not { } deployment) return (null, T("Something went wrong. Try again."));
        try
        {
            var made = await deployment.ArrangeMeeting(room, at.ToUnixTimeMilliseconds(), relay ?? "");
            _reads++;
            Arrangements = Arrangements.Append(made).OrderBy(a => a.At, StringComparer.Ordinal).ToList();
            State = Status.Ready;
            Notify();
            return (made, null);
        }
        catch (Exception fault)
        {
            return (null, Refusals.ExplainArranging(Faults.Of(fault)));
        }
    }

    /// <summary>Calls one off; the sentence to say when that did not take.</summary>
    public async Task<string?> Cancel(Arrangement one)
    {
        if (_app.Deployment is not { } deployment) return null;
        try
        {
            await deployment.CancelMeeting(one.Id);
            _reads++;
            Arrangements = Arrangements.Where(a => a.Id != one.Id).ToList();
            Notify();
            return null;
        }
        catch (Exception fault)
        {
            return Refusals.ExplainArranging(Faults.Of(fault));
        }
    }

    /// <summary>meeting.ts <c>linkFor</c>: the page, the meeting's token, the room; the core writes it as the web does.</summary>
    public string? Link(Arrangement one) =>
        _app.Deployment is { } deployment && one.Token is { } token ? deployment.MeetingLink(token, one.Room) : null;

    /// <summary>calendar.ts <c>icsFor</c>: the meeting as a calendar file's text.</summary>
    public string? CalendarText(Arrangement one, DateTimeOffset? now = null)
    {
        if (Link(one) is not { } link || Refusals.Instant(one.At) is not { } start) return null;
        static string Stamp(DateTimeOffset at) => at.UtcDateTime.ToString("yyyyMMdd'T'HHmmss'Z'", CultureInfo.InvariantCulture);
        static string Escape(string text) => text.Replace("\\", "\\\\").Replace(";", "\\;").Replace(",", "\\,").Replace("\n", "\\n");
        var lines = new[]
        {
            "BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Tomoshibi//Arranged meeting//EN", "CALSCALE:GREGORIAN",
            "METHOD:PUBLISH", "BEGIN:VEVENT", $"UID:{one.Id}@tomoshibi", $"DTSTAMP:{Stamp(now ?? DateTimeOffset.UtcNow)}",
            $"DTSTART:{Stamp(start)}", $"SUMMARY:{Escape(one.Room)}", $"URL:{link}",
            $"DESCRIPTION:{Escape(link)}", "END:VEVENT", "END:VCALENDAR",
        };
        return string.Join("\r\n", lines) + "\r\n";
    }

    // Rooms lately

    /// <summary>
    /// A room this device joined, kept so it can be joined again in one
    /// press: a call app's recent list, which the web has no place for. Kept
    /// on this device only and per deployment, and never anything but the
    /// name and when.
    /// </summary>
    public sealed record Recent(string Room, string Server, DateTimeOffset At)
    {
        public string Id => Server + "#" + Room;
    }

    public IReadOnlyList<Recent> Recents { get; private set; }

    /// <summary>The ones joined on the deployment showing now, latest first.</summary>
    public IReadOnlyList<Recent> RecentHere
    {
        get
        {
            var here = _app.Deployment?.BaseUrl() ?? "";
            return Recents.Where(r => r.Server == here).OrderByDescending(r => r.At).ToList();
        }
    }

    public void Remember(string room)
    {
        if (_app.Deployment?.BaseUrl() is not { } server || _app.Rehearsal) return;
        Recents = new[] { new Recent(room, server, DateTimeOffset.UtcNow) }
            .Concat(Recents.Where(r => !(r.Server == server && r.Room == room)))
            .Take(RecentsKept).ToList();
        SaveRecents();
        Notify();
    }

    /// <summary>Takes a room off the recent list, which is the person's own record of where they have been and theirs to tidy.</summary>
    public void Forget(Recent recent)
    {
        Recents = Recents.Where(r => r.Id != recent.Id).ToList();
        if (!_app.Rehearsal) SaveRecents();
        Notify();
    }

    private void SaveRecents()
    {
        var array = new JsonArray();
        foreach (var recent in Recents)
        {
            array.Add(new JsonObject { ["room"] = recent.Room, ["server"] = recent.Server, ["at"] = recent.At.ToUnixTimeMilliseconds() });
        }
        _defaults.Set(RecentsKey, array.ToJsonString());
    }

    private IReadOnlyList<Recent> LoadRecents()
    {
        try
        {
            if (_defaults.String(RecentsKey) is not { } stored || JsonNode.Parse(stored) is not JsonArray array) return Array.Empty<Recent>();
            var recents = new List<Recent>();
            foreach (var item in array)
            {
                if (item is not JsonObject one) continue;
                var room = one["room"]?.GetValue<string>();
                var server = one["server"]?.GetValue<string>();
                if (string.IsNullOrEmpty(room) || string.IsNullOrEmpty(server)) continue;
                var at = one["at"] is JsonValue value && value.TryGetValue<long>(out var ms) ? ms : 0;
                recents.Add(new Recent(room, server, DateTimeOffset.FromUnixTimeMilliseconds(at)));
            }
            return recents;
        }
        catch (Exception)
        {
            return Array.Empty<Recent>();
        }
    }

    // The relays from here

    /// <summary>Round trips by relay name, null until measured; a relay that did not answer is present with no number.</summary>
    public IReadOnlyDictionary<string, double?>? Measured { get; private set; }
    public bool Measuring { get; private set; }
    /// <summary>The order the first measurement settled on, so a row does not move out from under a pointer already reaching for it.</summary>
    public IReadOnlyList<string> Order { get; private set; } = Array.Empty<string>();

    public async Task Measure()
    {
        if (Measuring) return;
        if (_app.Relays?.Relays is not { Length: > 0 } relays) return;
        Measuring = true;
        Notify();
        try
        {
            var timings = await TomoshibiProtocolMethods.RelayTimings(relays);
            Measured = timings.ToDictionary(t => t.Name, t => t.Millis);
            if (Order.Count == 0) Order = timings.OrderBy(t => t.Millis ?? double.PositiveInfinity).Select(t => t.Name).ToList();
        }
        finally
        {
            Measuring = false;
            Notify();
        }
    }

    /// <summary>The relays under their region, the region's words said in the reader's language.</summary>
    public sealed record RegionGroup(string Region, IReadOnlyList<Relay> Relays);

    /// <summary>The relays under their region, in the settled order.</summary>
    public IReadOnlyList<RegionGroup> Grouped()
    {
        var relays = _app.Relays?.Relays ?? Array.Empty<Relay>();
        var order = Order;
        var ranked = order.Count == 0 ? relays.ToList() : relays.OrderBy(r => order.ToList().IndexOf(r.Name) is var at && at < 0 ? int.MaxValue : at).ToList();
        var groups = new List<(string Region, List<Relay> Relays)>();
        foreach (var relay in ranked)
        {
            var region = string.Join(" · ", (relay.Region ?? "").Split('/').Select(p => p.Trim()).Where(p => p.Length > 0).Select(Say));
            var group = groups.FindIndex(g => g.Region == region);
            if (group >= 0) groups[group].Relays.Add(relay);
            else groups.Add((region, new List<Relay> { relay }));
        }
        return groups.Select(g => new RegionGroup(g.Region, g.Relays)).ToList();
    }

    /// <summary>The fastest relay that answered, and its round trip.</summary>
    public sealed record Fastest(string Name, double Millis);

    /// <summary>
    /// The fastest relay that answered, which is what "automatic" settles on.
    /// Never one kept for administrators, and never one out of service
    /// (relays.ts <c>fastest</c>): the join measures without them.
    /// </summary>
    public Fastest? FastestRelay
    {
        get
        {
            if (Measured is not { } measured) return null;
            var usable = (_app.Relays?.Relays ?? Array.Empty<Relay>()).Where(r => !r.AdminOnly && !r.Maintenance).Select(r => r.Name).ToHashSet();
            return measured.Where(p => usable.Contains(p.Key) && p.Value != null)
                .Select(p => new Fastest(p.Key, p.Value!.Value))
                .OrderBy(f => f.Millis).FirstOrDefault();
        }
    }

    public string Label(string name)
    {
        var relay = _app.Relays?.Relays.FirstOrDefault(r => r.Name == name);
        return relay == null ? name : Say(relay.Label ?? relay.Name);
    }

    /// <summary>
    /// meeting.ts <c>whenSaid</c>: the date, the time and the zone, in the
    /// reader's language and the reader's own zone.
    /// </summary>
    public static string Said(DateTimeOffset at)
    {
        // The date at medium length and the time short, as ICU writes them
        // on Android, from Windows' own formatter in the reader's language.
        var local = at.ToLocalTime();
        return UI.Kit.Times.Written(at, "month.abbreviated day year hour minute") + " " + Zone(local);
    }

    /// <summary>The short name of the reader's zone at that moment, as a browser writes it: UTC and the offset, there being no short names on Windows.</summary>
    public static string Zone(DateTimeOffset local)
    {
        var offset = local.Offset;
        if (offset == TimeSpan.Zero) return "UTC";
        var sign = offset < TimeSpan.Zero ? "-" : "+";
        var whole = offset.Duration();
        return whole.Minutes == 0 ? $"GMT{sign}{whole.Hours}" : $"GMT{sign}{whole.Hours}:{whole.Minutes:00}";
    }
}
