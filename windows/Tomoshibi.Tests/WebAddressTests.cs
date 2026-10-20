using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/*
 * Addresses other people write, which the app loads (a song's cover, an
 * account's picture) or opens (a shown video's link). A participant who put
 * "https://" as a song's cover closed every Windows client with the song
 * desk open, because the string was handed to `new Uri` on the UI thread.
 */
public class WebAddressTests
{
    [Theory]
    [InlineData("https://")]
    [InlineData("http://")]
    [InlineData("https://example.com:port/cover.jpg")]
    [InlineData("https://exa mple.com/cover.jpg")]
    [InlineData("http")]
    [InlineData("httpcover.jpg")]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData(null)]
    public void WhatOnlyStartsLikeAnAddressIsNone(string? written) => Assert.Null(WebAddress.Of(written));

    [Theory]
    [InlineData("javascript:alert(1)")]
    [InlineData("file:///C:/Windows/win.ini")]
    [InlineData("ms-settings:privacy-microphone")]
    [InlineData("tomoshibi://example.com/#/room")]
    [InlineData("/covers/1.jpg")]
    public void NothingButTheWebIsOne(string written) => Assert.Null(WebAddress.Of(written));

    [Theory]
    [InlineData("https://example.com/cover.jpg", "https://example.com/cover.jpg")]
    [InlineData("http://10.211.55.2:18790/music/cover?id=1", "http://10.211.55.2:18790/music/cover?id=1")]
    [InlineData("  https://example.com/a b.jpg  ", "https://example.com/a%20b.jpg")]
    [InlineData("https://www.bilibili.com/video/BV1xx?p=2", "https://www.bilibili.com/video/BV1xx?p=2")]
    public void AWholeAddressIsItself(string written, string meant) => Assert.Equal(meant, WebAddress.Of(written)!.AbsoluteUri);
}
