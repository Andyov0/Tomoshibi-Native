using System.Text.Json;
using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/*
 * The sound panel's record, held to what the web writes and reads under the
 * same name (web/src/live/hearing.ts and hearing.test.ts), case for case as
 * Android's HearingTest holds it. What these guard is a setting quietly not
 * being what somebody chose: a guest's block surviving onto whoever is
 * issued that identity next, a hand-edited volume of 3 played three times
 * too loud, a book the web wrote that the app cannot read.
 */
public class HearingTests
{
    private static readonly HearingFiling Proven = new("ABCDEFGHIJ", Keeps: true);
    private static readonly HearingFiling Guest = new("g0123456789abcdef0123456789abcdef", Keeps: false);

    [Fact]
    public void EverybodyIsAsSentUntilSomebodySaysOtherwise()
    {
        var book = new HearingBook();
        foreach (var sound in Sounds.All) Assert.Equal(HearingSetting.AsSent, book.Setting(Proven, sound));
        Assert.Null(book.Stored);
    }

    [Fact]
    public void AProvenSettingIsKeptInTheWebsShape()
    {
        var book = new HearingBook();
        Assert.True(book.Set(Proven, Sound.Music, new HearingSetting(0.25, false)));
        Assert.True(book.Set(Proven, Sound.Voice, new HearingSetting(1.0, true)));
        var stored = book.Stored!;
        // Byte for byte what the web's JSON.stringify writes for the same book.
        Assert.Equal("""{"ABCDEFGHIJ/music":{"volume":0.25,"blocked":false},"ABCDEFGHIJ/voice":{"volume":1,"blocked":true}}""", stored);
        using var parsed = JsonDocument.Parse(stored);
        Assert.Equal(0.25, parsed.RootElement.GetProperty("ABCDEFGHIJ/music").GetProperty("volume").GetDouble());
        // And read back as it was written, by the next launch.
        var again = new HearingBook(stored);
        Assert.Equal(new HearingSetting(0.25, false), again.Setting(Proven, Sound.Music));
        Assert.Equal(new HearingSetting(1.0, true), again.Setting(Proven, Sound.Voice));
        Assert.Equal(HearingSetting.AsSent, again.Setting(Proven, Sound.Screen));
    }

    [Fact]
    public void AVolumeIsWrittenInItsShortestForm()
    {
        var book = new HearingBook();
        book.Set(Proven, Sound.Voice, new HearingSetting(0.35, false));
        Assert.Equal("""{"ABCDEFGHIJ/voice":{"volume":0.35,"blocked":false}}""", book.Stored);
        book.Set(Proven, Sound.Voice, new HearingSetting(0.0, false));
        Assert.Equal("""{"ABCDEFGHIJ/voice":{"volume":0,"blocked":false}}""", book.Stored);
        // As JavaScript writes it, not as .NET's 5E-04.
        book.Set(Proven, Sound.Voice, new HearingSetting(0.0005, false));
        Assert.Equal("""{"ABCDEFGHIJ/voice":{"volume":0.0005,"blocked":false}}""", book.Stored);
    }

    [Fact]
    public void AGuestsSettingLastsTheCallAndIsNeverWritten()
    {
        var book = new HearingBook();
        Assert.False(book.Set(Guest, Sound.Voice, new HearingSetting(0.0, true)));
        Assert.Equal(new HearingSetting(0.0, true), book.Setting(Guest, Sound.Voice));
        Assert.Null(book.Stored);
    }

    [Fact]
    public void TurningSomebodyBackUpRemovesThemFromStorage()
    {
        var book = new HearingBook();
        book.Set(Proven, Sound.Voice, new HearingSetting(0.5, false));
        Assert.NotNull(book.Stored);
        Assert.True(book.Set(Proven, Sound.Voice, HearingSetting.AsSent));
        Assert.Null(book.Stored);
    }

    [Fact]
    public void WhatTheWebWroteIsReadAndWhatWasEditedByHandIsClamped()
    {
        var book = new HearingBook("""
            {"AAAAAAAAAA/voice":{"volume":3,"blocked":false},
             "BBBBBBBBBB/voice":{"volume":-1},
             "CCCCCCCCCC/voice":{"volume":"loud","blocked":1},
             "DDDDDDDDDD/voice":{"volume":false,"blocked":true},
             "EEEEEEEEEE/voice":{"volume":0.4,"blocked":true}}
            """);
        HearingSetting Setting(string key) => book.Setting(new HearingFiling(key, true), Sound.Voice);
        Assert.Equal(new HearingSetting(1.0, false), Setting("AAAAAAAAAA"));
        Assert.Equal(new HearingSetting(0.0, false), Setting("BBBBBBBBBB"));
        // A string is not a volume and a 1 is not a block, as `=== true` has it.
        Assert.Equal(new HearingSetting(1.0, false), Setting("CCCCCCCCCC"));
        // JSON's false is not a volume of nought either.
        Assert.Equal(new HearingSetting(1.0, true), Setting("DDDDDDDDDD"));
        Assert.Equal(new HearingSetting(0.4, true), Setting("EEEEEEEEEE"));
    }

    [Fact]
    public void UnreadableStorageIsNothingDecided()
    {
        Assert.Equal(new HearingBook(), new HearingBook("not json"));
        Assert.Equal(new HearingBook(), new HearingBook("[1,2]"));
        Assert.Equal(new HearingBook(), new HearingBook(null));
    }

    [Fact]
    public void AVolumeOutsideTheRangeIsClampedOnTheWayIn()
    {
        var book = new HearingBook();
        book.Set(Proven, Sound.Screen, new HearingSetting(7.0, false));
        // Clamped to 1 and not blocked: as sent, so nothing is kept at all.
        Assert.Equal(HearingSetting.AsSent, book.Setting(Proven, Sound.Screen));
        Assert.Null(book.Stored);
    }

    [Fact]
    public void SilencedIsABlockOrNothingToHear()
    {
        Assert.True(new HearingSetting(0.0, false).Silenced);
        Assert.True(new HearingSetting(1.0, true).Silenced);
        Assert.False(new HearingSetting(0.05, false).Silenced);
    }
}
