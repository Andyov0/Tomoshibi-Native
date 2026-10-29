using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// What the call is told of the machine's internet connection. The core
/// resumes the call when the route it is told differs from the one before,
/// so what these guard is a route that changes when the connection has not
/// (a resume for nothing, the media stalling, every time Windows raises its
/// event) and one that stays the same when another adapter has taken over.
/// </summary>
public class NetworkRouteTests
{
    [Fact]
    public void TheSameAdapterOnTheInternetIsTheSameRouteWhateverTheLevel()
    {
        var one = NetworkRoute.Of("InternetAccess", "3f6c");
        Assert.Equal(new NetworkRoute(true, "adapter 3f6c"), one);
        Assert.Equal(one, NetworkRoute.Of("ConstrainedInternetAccess", "3f6c"));
    }

    [Fact]
    public void AnotherAdapterIsAnotherRoute()
    {
        Assert.NotEqual(NetworkRoute.Of("InternetAccess", "3f6c"), NetworkRoute.Of("InternetAccess", "91aa"));
    }

    [Fact]
    public void NoProfileOrNoInternetIsOffline()
    {
        Assert.Equal(new NetworkRoute(false, ""), NetworkRoute.Of(null, null));
        Assert.Equal(new NetworkRoute(false, ""), NetworkRoute.Of("None", "3f6c"));
        Assert.Equal(new NetworkRoute(false, ""), NetworkRoute.Of("LocalAccess", "3f6c"));
    }
}
