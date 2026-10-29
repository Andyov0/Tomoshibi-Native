namespace Tomoshibi.Basics;

/// <summary>
/// The machine's internet connection as the call is told about it (the
/// core's <c>NetworkPath</c>): whether there is one, and which adapter it
/// goes through.
///
/// The core decides from these whether the connection really changed, after
/// letting a burst of reports settle, and only then makes the call resume or
/// retry (protocol network.rs), because Windows raises its network status
/// event far more often than the connection changes: several times as an
/// adapter comes up, and on changes that have nothing to do with the
/// internet connection at all. What it needs from here is a route that is
/// equal for the same connection and different for another one.
/// </summary>
public readonly record struct NetworkRoute(bool Online, string Route)
{
    /// <summary>
    /// The internet connection profile read as it stands: its connectivity
    /// level's name (<c>NetworkConnectivityLevel</c>) and its adapter's id,
    /// both absent where there is no profile. Only a level that reaches the
    /// internet is online; a local network alone does not carry a call to the
    /// deployment, and becoming one that does is the change to act on.
    /// </summary>
    public static NetworkRoute Of(string? level, string? adapter) =>
        level is "InternetAccess" or "ConstrainedInternetAccess"
            ? new NetworkRoute(true, "adapter " + (adapter ?? ""))
            : new NetworkRoute(false, "");
}
