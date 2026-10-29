using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;
using Windows.Networking.Connectivity;

namespace Tomoshibi.Model;

/// <summary>
/// Tells the call what the machine's internet connection is, every time
/// Windows raises its network status event (<c>Call.PathSeen</c>), for as
/// long as the call lasts.
///
/// Without it a call on a connection that went away noticed only when the
/// server's ping went unanswered, a call gone quiet for that long, and a call
/// already reconnecting when the connection came back sat out the rest of its
/// wait, up to eight seconds, before it tried; the web resumes or retries the
/// moment the browser says it is online. The event fires many times for one
/// change and for changes to other adapters, so every report is passed on as
/// it comes and the core does the rest: it waits for a burst to settle and
/// acts only on a real change of connection (NetworkRoute, protocol
/// network.rs), each one it acts on being a resume of a call that may have
/// had nothing wrong with it.
/// </summary>
internal sealed class NetworkWatch : IDisposable
{
    private readonly WeakReference<Call> _call;
    private bool _started;

    public NetworkWatch(Call call) => _call = new WeakReference<Call>(call);

    public void Start()
    {
        if (_started) return;
        _started = true;
        NetworkInformation.NetworkStatusChanged += Changed;
        // What the connection is now, as the first report: the core takes it
        // as the baseline and says nothing.
        Changed(null);
    }

    private void Changed(object? sender)
    {
        NetworkRoute route;
        try
        {
            var profile = NetworkInformation.GetInternetConnectionProfile();
            route = NetworkRoute.Of(profile?.GetNetworkConnectivityLevel().ToString(), profile?.NetworkAdapter?.NetworkAdapterId.ToString());
        }
        catch (Exception fault)
        {
            Log.Line($"network path unreadable: {fault.Message}");
            return;
        }
        Log.Line($"network path: online={route.Online} route={route.Route}");
        if (_call.TryGetTarget(out var call)) call.PathSeen(new NetworkPath(route.Online, route.Route));
    }

    public void Dispose()
    {
        if (!_started) return;
        _started = false;
        NetworkInformation.NetworkStatusChanged -= Changed;
    }
}
