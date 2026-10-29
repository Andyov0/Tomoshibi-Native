using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// The guest identity and its proof, kept together. What these guard: a
/// proof sent beside an identity it was not issued with (the server would
/// issue a fresh identity, and a guest would lose its place and any pin), a
/// proof left behind when a passphrase join replaced the identity, and the
/// identity no longer being found where earlier versions of the app kept it.
/// </summary>
public class KeptIdentityTests
{
    private const string Base = "http://127.0.0.1:8080/";

    [Fact]
    public void AGuestsIdentityComesBackWithItsProof()
    {
        var defaults = new MemoryDefaults();
        Assert.Equal(new KeptIdentity(), KeptIdentity.Read(defaults, Base));
        KeptIdentity.Keep(new KeptIdentity("g1", "p1"), defaults, Base);
        Assert.Equal(new KeptIdentity("g1", "p1"), KeptIdentity.Read(defaults, Base));
    }

    [Fact]
    public void AnAnswerWithoutAProofForgetsTheOldOne()
    {
        var defaults = new MemoryDefaults();
        KeptIdentity.Keep(new KeptIdentity("g1", "p1"), defaults, Base);
        KeptIdentity.Keep(new KeptIdentity("s1", null), defaults, Base);
        Assert.Equal(new KeptIdentity("s1", null), KeptIdentity.Read(defaults, Base));
        KeptIdentity.Keep(new KeptIdentity("g2", ""), defaults, Base);
        Assert.Null(KeptIdentity.Read(defaults, Base).Proof);
    }

    [Fact]
    public void TheIdentityIsWhereEarlierVersionsKeptItAndEachDeploymentHasItsOwn()
    {
        var defaults = new MemoryDefaults();
        defaults.Set("tomoshibi.identity." + Base, "old");
        Assert.Equal(new KeptIdentity("old", null), KeptIdentity.Read(defaults, Base));
        KeptIdentity.Keep(new KeptIdentity("g1", "p1"), defaults, "https://meet.example.com/");
        Assert.Equal(new KeptIdentity("old", null), KeptIdentity.Read(defaults, Base));
    }

    [Fact]
    public void AProofWithoutItsIdentityIsNeverRead()
    {
        var defaults = new MemoryDefaults();
        defaults.Set(KeptIdentity.Keys(Base).Proof, "stray");
        Assert.Equal(new KeptIdentity(), KeptIdentity.Read(defaults, Base));
    }
}
