using System.Globalization;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.Basics;

/// <summary>
/// Why something the deployment was asked did not happen, as the screens
/// need to know it: the server did not answer, it answered with a refusal
/// and its code, or something else broke. The app makes one of these from
/// the core's ProtocolException; kept here, apart from the core's bindings,
/// so that the sentences below are tested without the core.
/// </summary>
public abstract record Failure
{
    public sealed record Network : Failure;
    public sealed record Refused(string Code) : Failure;
    public sealed record Other : Failure;
}

/// <summary>
/// Why the last call ended (App.tsx, the <c>ended</c> handler): three endings
/// that look alike from the inside, a call that simply stopped, and want
/// three different sentences. Somebody told only "You left the call." after
/// the host closed the room reads the call as having broken.
/// </summary>
public enum Ending
{
    /// <summary>They pressed leave.</summary>
    Left,
    /// <summary>The room was closed for everybody (ROOM_DELETED, which the web calls DISCONNECT_ROOM_CLOSED).</summary>
    Closed,
    /// <summary>They alone were taken out (PARTICIPANT_REMOVED).</summary>
    Removed,
    /// <summary>Anything else: the connection failed and the client gave up.</summary>
    Lost,
}

/// <summary>
/// What the server's refusals read as (api.ts <c>explain</c>, Account.tsx,
/// Schedule.tsx), the same sentences as Android's AppModel and LobbyModel.
/// The server sends codes and the client owns the sentence.
/// </summary>
public static class Refusals
{
    /// <summary>api.ts <c>explain</c>, for the room the person was trying; <paramref name="scope"/> is the core's scope of that room.</summary>
    public static string Explain(Failure failure, string room, string scope) => failure switch
    {
        Failure.Network => T("Could not reach the server."),
        Failure.Refused { Code: var code } => code switch
        {
            "rate_limited" => T("Too many attempts. Try again in a moment."),
            "invalid_room" => T("Room names can only use lowercase letters, numbers and dashes."),
            "room_not_open" => T("{room} isn't open. Ask the organiser for the link.", ("room", room)),
            "not_invited" => T("{room} is by invitation. Ask the organiser for a link.", ("room", room)),
            "not_in_scope" => T("Only members of {scope} can join {room}.", ("room", room), ("scope", scope)),
            "relay_not_allowed" => T("Access denied. That server is for administrators."),
            "blocked" => T("This passphrase cannot join. Ask the organiser."),
            "not_yours" => T("That name and passphrase do not go together."),
            "server_error" => T("Something went wrong. Try again."),
            _ => T("Could not join {room}.", ("room", room)),
        },
        _ => T("Something went wrong. Try again."),
    };

    /// <summary>Whether the problem shown is one of the two refusals a knock answers (PreJoin.tsx ~521).</summary>
    public static bool CanKnock(string? problem, string room, string scope)
    {
        if (problem == null) return false;
        return problem == T("{room} is by invitation. Ask the organiser for a link.", ("room", room)) ||
            problem == T("Only members of {scope} can join {room}.", ("room", room), ("scope", scope));
    }

    /// <summary>Account.tsx <c>explainLink</c>.</summary>
    public static string ExplainLink(Failure failure)
    {
        if (failure is not Failure.Refused { Code: var code }) return T("That link could not be made. Try again.");
        return code switch
        {
            "not_yours" => T("You can make links only into rooms of your own scopes."),
            "name_too_long" => T("That name is too long."),
            "bad_time" => T("That window cannot be used. It has to end after it starts, and after now."),
            "invalid_room" => T("Room names can only use lowercase letters, numbers and dashes."),
            "rate_limited" => T("Too many attempts. Try again in a moment."),
            "not_allowed" => T("Guest links have not been opened to you."),
            _ => T("That link could not be made. Try again."),
        };
    }

    /// <summary>Account.tsx <c>Passphrase</c>'s <c>explain</c>.</summary>
    public static string ExplainPassphrase(Failure failure)
    {
        if (failure is not Failure.Refused { Code: var code }) return T("That could not be changed. Try again.");
        return code switch
        {
            "not_yours" => T("That is not your current passphrase."),
            "passphrase_too_short" => T("At least eight characters."),
            "passphrase_unchanged" => T("That is the passphrase you already have."),
            "passphrase_in_use" => T("Somebody else already uses that passphrase. Choose another."),
            _ => T("That could not be changed. Try again."),
        };
    }

    /// <summary>Schedule.tsx <c>explainArranging</c>.</summary>
    public static string ExplainArranging(Failure failure)
    {
        if (failure is not Failure.Refused { Code: var code }) return T("Something went wrong. Try again.");
        return code switch
        {
            "room_already_arranged" => T("That room already has a meeting arranged."),
            "bad_time" => T("That time is not one a meeting can be arranged for."),
            "meetings_need_invites" => T("Meetings here ask for an account, so a link cannot let anybody in."),
            "not_yours" => T("That is somebody else's room."),
            "too_many_meetings" => T("You have as many meetings arranged as you can have."),
            "invalid_room" => T("Room names can only use lowercase letters, numbers and dashes."),
            "scoped_room" => T("A room held under a scope cannot be arranged ahead. Send a link with a time on it instead."),
            "relay_not_allowed" => T("Access denied. That server is for administrators."),
            "not_signed_in" => T("Your session has expired. Sign in again."),
            "not_allowed" => T("Arranging meetings has not been opened to you."),
            _ => T("Something went wrong. Try again."),
        };
    }

    /// <summary>How a call that ended for the media server's reason reads to the person (App.tsx, the <c>ended</c> handler).</summary>
    public static Ending EndingFor(string reason)
    {
        if (reason.Contains("RoomDeleted") || reason.Contains("RoomClosed")) return Ending.Closed;
        if (reason.Contains("ParticipantRemoved")) return Ending.Removed;
        if (reason.Contains("ClientInitiated")) return Ending.Left;
        return Ending.Lost;
    }

    /// <summary>Whether a call that ended for that reason was taken away by the server (the room closed, or this participant removed).</summary>
    public static bool MovedAway(string reason) =>
        reason.Contains("RoomDeleted") || reason.Contains("RoomClosed") || reason.Contains("ParticipantRemoved");

    /// <summary>An instant the server wrote, with or without fractions of a second, or null.</summary>
    public static DateTimeOffset? Instant(string? text)
    {
        if (string.IsNullOrEmpty(text)) return null;
        return DateTimeOffset.TryParse(text, CultureInfo.InvariantCulture, DateTimeStyles.AssumeUniversal, out var at) ? at : null;
    }

    /// <summary>What a person is called in a notice: their name, or their identity when they gave none (notices.ts <c>named</c>).</summary>
    public static string Named(string name, string identity) => string.IsNullOrEmpty(name) ? identity : name;
}

/// <summary>
/// The parts of a link opened with the app that say where it goes: the
/// origin it is for, and the room, invitation and arranged meeting it
/// carries (<c>https://&lt;deployment&gt;/?invite=T#/room</c>,
/// <c>?meeting=T#/room</c>, or the same under the <c>tomoshibi:</c> scheme;
/// Android's model/AppModel.kt <c>OpenedLink</c>).
/// </summary>
public sealed record OpenedLink(string Origin, string Room, string? Invite, string? Meeting)
{
    /// <summary>
    /// Whether this is a link into the deployment at <paramref name="baseUrl"/>:
    /// compared without the trailing slash the deployment's address always
    /// has and a link's origin never does, and without a default port.
    /// </summary>
    public bool IsFor(string baseUrl) => Origin == baseUrl.TrimEnd('/');

    public static OpenedLink? Parse(string link)
    {
        if (!Uri.TryCreate(link.Trim(), UriKind.Absolute, out var uri)) return null;
        var scheme = uri.Scheme.ToLowerInvariant();
        if (scheme == "tomoshibi") scheme = "https";
        if (string.IsNullOrEmpty(uri.Host)) return null;
        var host = uri.Host.ToLowerInvariant();
        var port = uri.IsDefaultPort || uri.Port < 0 || (scheme == "https" && uri.Port == 443) || (scheme == "http" && uri.Port == 80) ? (int?)null : uri.Port;
        var origin = $"{scheme}://{host}" + (port is { } p ? $":{p}" : "");
        var fragment = uri.Fragment.StartsWith('#') ? uri.Fragment[1..] : uri.Fragment;
        var room = fragment.StartsWith('/') ? fragment[1..] : fragment;
        var query = uri.Query.TrimStart('?').Split('&', StringSplitOptions.RemoveEmptyEntries).Select(item =>
        {
            var at = item.IndexOf('=');
            return (Key: Decode(at < 0 ? item : item[..at]), Value: at < 0 ? "" : Decode(item[(at + 1)..]));
        }).ToList();
        string? First(string name) => query.Where(q => q.Key == name).Select(q => q.Value).FirstOrDefault();
        return new OpenedLink(origin, Uri.UnescapeDataString(room), First("invite"), First("meeting"));
    }

    /// <summary>Percent-decoding as a URL's query items are read: a plus is a plus, not a space.</summary>
    private static string Decode(string text)
    {
        try
        {
            return Uri.UnescapeDataString(text);
        }
        catch (UriFormatException)
        {
            return text;
        }
    }
}
