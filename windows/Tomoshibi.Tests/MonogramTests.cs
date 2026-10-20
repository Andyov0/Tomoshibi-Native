using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/// <summary>
/// A person's monogram is one colour, and the same one on every platform:
/// these are the answers MonogramTests.swift and Android's MonogramTest hold
/// their ports to, so a hash that drifts here puts somebody in another colour
/// on Windows and fails first.
/// </summary>
public class MonogramTests
{
    [Theory]
    [InlineData("", 1)]
    [InlineData("a", 0)]
    [InlineData("andy", 5)]
    [InlineData("Andy", 1)]
    [InlineData("guest-7f3a9c", 1)]
    [InlineData("sig:Zq3n0Vf8", 2)]
    [InlineData("张伟", 3)]
    [InlineData("田中さん", 4)]
    [InlineData("😀x", 0)]
    [InlineData("0f9e2d4c-8b1a-4f6e-9c3d-2a7b5e1f0c84", 2)]
    [InlineData("participant-1", 0)]
    [InlineData("participant-2", 5)]
    [InlineData("participant-3", 4)]
    public void TheColourAgreesWithTheIPhoneAndAndroid(string identity, int index) => Assert.Equal(index, Monograms.Index(identity, 6));
}
