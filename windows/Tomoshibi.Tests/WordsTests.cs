using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/*
 * What the server's refusals read as (api.ts `explain`, Account.tsx,
 * Schedule.tsx), how a call's end is told apart, and the links the app
 * opens; Android's WordsTest, case for case. What these guard is somebody
 * being told the wrong next move: "Could not join" where the room was only
 * by invitation, "You left" after the host closed the room, a link to
 * another server followed.
 */
public class WordsTests
{
    private static Failure Refused(string code) => new Failure.Refused(code);

    [Fact]
    public void TheServersRefusalsReadAsTheWebSaysThem()
    {
        const string room = "team-room";
        Assert.Equal("Could not reach the server.", Refusals.Explain(new Failure.Network(), room, ""));
        Assert.Equal("Too many attempts. Try again in a moment.", Refusals.Explain(Refused("rate_limited"), room, ""));
        Assert.Equal("Room names can only use lowercase letters, numbers and dashes.", Refusals.Explain(Refused("invalid_room"), room, ""));
        Assert.Equal("team-room isn't open. Ask the organiser for the link.", Refusals.Explain(Refused("room_not_open"), room, ""));
        Assert.Equal("team-room is by invitation. Ask the organiser for a link.", Refusals.Explain(Refused("not_invited"), room, ""));
        Assert.Equal("Access denied. That server is for administrators.", Refusals.Explain(Refused("relay_not_allowed"), room, ""));
        Assert.Equal("This passphrase cannot join. Ask the organiser.", Refusals.Explain(Refused("blocked"), room, ""));
        Assert.Equal("That name and passphrase do not go together.", Refusals.Explain(Refused("not_yours"), room, ""));
        Assert.Equal("Something went wrong. Try again.", Refusals.Explain(Refused("server_error"), room, ""));
        // A code nobody has a sentence for is about the room they tried.
        Assert.Equal("Could not join team-room.", Refusals.Explain(Refused("something_new"), room, ""));
        // Anything that is not the server answering is not about the room.
        Assert.Equal("Something went wrong. Try again.", Refusals.Explain(new Failure.Other(), room, ""));
        // The scope, which the core works out, is put where the sentence names it.
        Assert.Equal("Only members of team can join team.room.", Refusals.Explain(Refused("not_in_scope"), "team.room", "team"));
    }

    [Fact]
    public void AKnockIsOfferedOnlyWhereTheRoomWasByInvitation()
    {
        const string room = "team-room";
        Assert.True(Refusals.CanKnock(Refusals.Explain(Refused("not_invited"), room, ""), room, ""));
        Assert.True(Refusals.CanKnock(Refusals.Explain(Refused("not_in_scope"), "team.room", "team"), "team.room", "team"));
        Assert.False(Refusals.CanKnock(Refusals.Explain(Refused("blocked"), room, ""), room, ""));
        Assert.False(Refusals.CanKnock(null, room, ""));
    }

    [Fact]
    public void GuestLinkAndPassphraseRefusalsHaveTheirOwnSentences()
    {
        Assert.Equal("You can make links only into rooms of your own scopes.", Refusals.ExplainLink(Refused("not_yours")));
        Assert.Equal("That name is too long.", Refusals.ExplainLink(Refused("name_too_long")));
        Assert.Equal("That window cannot be used. It has to end after it starts, and after now.", Refusals.ExplainLink(Refused("bad_time")));
        Assert.Equal("Guest links have not been opened to you.", Refusals.ExplainLink(Refused("not_allowed")));
        Assert.Equal("That link could not be made. Try again.", Refusals.ExplainLink(new Failure.Network()));

        Assert.Equal("That is not your current passphrase.", Refusals.ExplainPassphrase(Refused("not_yours")));
        Assert.Equal("At least eight characters.", Refusals.ExplainPassphrase(Refused("passphrase_too_short")));
        Assert.Equal("That is the passphrase you already have.", Refusals.ExplainPassphrase(Refused("passphrase_unchanged")));
        Assert.Equal("Somebody else already uses that passphrase. Choose another.", Refusals.ExplainPassphrase(Refused("passphrase_in_use")));
        Assert.Equal("That could not be changed. Try again.", Refusals.ExplainPassphrase(Refused("other")));
    }

    [Fact]
    public void ArrangingRefusalsReadAsTheSchedulePageSaysThem()
    {
        Assert.Equal("That room already has a meeting arranged.", Refusals.ExplainArranging(Refused("room_already_arranged")));
        Assert.Equal("Your session has expired. Sign in again.", Refusals.ExplainArranging(Refused("not_signed_in")));
        Assert.Equal(
            "A room held under a scope cannot be arranged ahead. Send a link with a time on it instead.",
            Refusals.ExplainArranging(Refused("scoped_room")));
        Assert.Equal("Something went wrong. Try again.", Refusals.ExplainArranging(Refused("nope")));
        Assert.Equal("Something went wrong. Try again.", Refusals.ExplainArranging(new Failure.Other()));
    }

    [Fact]
    public void ThreeEndingsThatLookAlikeFromInsideAreToldApart()
    {
        // The Rust SDK's name for the protocol's ROOM_DELETED; waiting only
        // for RoomClosed left the app in a call that had gone.
        Assert.Equal(Ending.Closed, Refusals.EndingFor("RoomDeleted"));
        Assert.Equal(Ending.Closed, Refusals.EndingFor("RoomClosed"));
        Assert.Equal(Ending.Removed, Refusals.EndingFor("ParticipantRemoved"));
        Assert.Equal(Ending.Left, Refusals.EndingFor("ClientInitiated"));
        Assert.Equal(Ending.Lost, Refusals.EndingFor("JoinFailure"));
        Assert.True(Refusals.MovedAway("RoomDeleted"));
        Assert.False(Refusals.MovedAway("ClientInitiated"));
    }

    [Fact]
    public void ALinkSaysWhereItGoes()
    {
        var link = OpenedLink.Parse("https://Meet.Example.com/?invite=a%2Bb#/team-room")!;
        Assert.Equal("https://meet.example.com", link.Origin);
        Assert.Equal("team-room", link.Room);
        Assert.Equal("a+b", link.Invite);
        Assert.Null(link.Meeting);
        Assert.True(link.IsFor("https://meet.example.com/"));
        Assert.False(link.IsFor("https://other.example.com/"));
        // The app's own scheme is the deployment's https.
        var own = OpenedLink.Parse("tomoshibi://meet.example.com/?meeting=T1#/standup")!;
        Assert.Equal("https://meet.example.com", own.Origin);
        Assert.Equal("T1", own.Meeting);
        // A plus in a query is a plus, not a space.
        Assert.Equal("a+b", OpenedLink.Parse("https://meet.example.com/?invite=a+b#/x")!.Invite);
        // A port that is not the default is part of the origin.
        Assert.Equal("http://10.0.0.2:18700", OpenedLink.Parse("http://10.0.0.2:18700/#/x")!.Origin);
        Assert.Null(OpenedLink.Parse("not a link"));
    }

    [Fact]
    public void AnInstantIsReadWithOrWithoutFractions()
    {
        Assert.Equal(new DateTimeOffset(2026, 10, 19, 12, 0, 0, TimeSpan.Zero), Refusals.Instant("2026-10-19T12:00:00Z"));
        Assert.Equal(new DateTimeOffset(2026, 10, 19, 12, 0, 0, 500, TimeSpan.Zero), Refusals.Instant("2026-10-19T12:00:00.500Z"));
        Assert.Null(Refusals.Instant("tomorrow"));
        Assert.Null(Refusals.Instant(null));
    }
}
