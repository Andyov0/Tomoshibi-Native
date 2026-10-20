namespace Tomoshibi.Basics;

/// <summary>
/// An address somebody else wrote, made into one the app can load or open,
/// or nothing (the web's own loads take whatever a string is and fail
/// quietly: an <c>img</c> with a broken <c>src</c> shows nothing).
///
/// A song's cover on the song desk is whatever the participant who added
/// the song said it was, and the desk's state is anybody's to send; a
/// shown video's link is its sharer's. <c>new Uri</c> throws for a string
/// that only starts like an address ("https://", a port that is not a
/// number), and it was called on the UI thread as the song began: every
/// Windows client with the song desk open closed at once, and its call with
/// it. Only a whole http or https address with a host is one.
/// </summary>
public static class WebAddress
{
    /// <summary>The address where <paramref name="text"/> is a whole http or https one with a host; otherwise null, and never an exception.</summary>
    public static Uri? Of(string? text)
    {
        if (string.IsNullOrWhiteSpace(text)) return null;
        if (!Uri.TryCreate(text.Trim(), UriKind.Absolute, out var uri)) return null;
        if (uri.Scheme != Uri.UriSchemeHttp && uri.Scheme != Uri.UriSchemeHttps) return null;
        return string.IsNullOrEmpty(uri.Host) ? null : uri;
    }
}
