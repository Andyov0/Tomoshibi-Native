using System.Runtime.InteropServices;
using Tomoshibi.Basics;
using Windows.Globalization.DateTimeFormatting;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// Times written in the reader's language: when a room was last joined
/// ("3 minutes ago", "yesterday"), and when a meeting is, the way Android
/// writes them through ICU. Windows carries ICU too (icu.dll, from Windows
/// 10 1903), so the relative phrases are ICU's own in every language rather
/// than a table of ours; where it cannot be reached the date is written
/// plainly instead.
/// </summary>
internal static class Times
{
    // ICU's C API as Windows exports it, without a version suffix.
    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    private static extern IntPtr ureldatefmt_open(string locale, IntPtr numberFormat, int width, int capitalization, ref int status);

    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl)]
    private static extern int ureldatefmt_format(IntPtr formatter, double offset, int unit, [Out] char[] result, int capacity, ref int status);

    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl)]
    private static extern int ureldatefmt_formatNumeric(IntPtr formatter, double offset, int unit, [Out] char[] result, int capacity, ref int status);

    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl)]
    private static extern void ureldatefmt_close(IntPtr formatter);

    private const int Year = 0, Month = 2, Week = 3, Day = 4, Hour = 5, Minute = 6, Second = 7;
    private const int Long = 0;
    private const int CapitalizationNone = 256;

    /// <summary>Android's <c>relativeNamed</c>: how long ago, in the steps a person thinks in.</summary>
    public static string Relative(DateTimeOffset at, DateTimeOffset? now = null)
    {
        var age = (now ?? DateTimeOffset.UtcNow) - at;
        var seconds = Math.Abs(age.TotalSeconds);
        var sign = age.TotalSeconds >= 0 ? -1 : 1;
        var days = seconds / 86400;
        try
        {
            if (seconds < 60) return Format(0, Second, numeric: false);
            if (seconds < 3600) return Format(sign * Math.Floor(seconds / 60), Minute, numeric: true);
            if (seconds < 86400) return Format(sign * Math.Floor(seconds / 3600), Hour, numeric: true);
            if (days < 2) return Format(sign, Day, numeric: false);
            if (days < 7) return Format(sign * Math.Floor(days), Day, numeric: true);
            if (days < 14) return Format(sign, Week, numeric: false);
            if (days < 30) return Format(sign * Math.Floor(days / 7), Week, numeric: true);
            if (days < 60) return Format(sign, Month, numeric: false);
            if (days < 365) return Format(sign * Math.Floor(days / 30), Month, numeric: true);
            if (days < 730) return Format(sign, Year, numeric: false);
            return Format(sign * Math.Floor(days / 365), Year, numeric: true);
        }
        catch (Exception)
        {
            return at.ToLocalTime().ToString("d", Words.Reading);
        }
    }

    private static string Format(double offset, int unit, bool numeric)
    {
        var status = 0;
        var locale = Localization.Shared.Language.Code().Replace('-', '_');
        var formatter = ureldatefmt_open(locale, IntPtr.Zero, Long, CapitalizationNone, ref status);
        if (status > 0 || formatter == IntPtr.Zero) throw new InvalidOperationException($"ICU {status}");
        try
        {
            var buffer = new char[128];
            var length = numeric
                ? ureldatefmt_formatNumeric(formatter, offset, unit, buffer, buffer.Length, ref status)
                : ureldatefmt_format(formatter, offset, unit, buffer, buffer.Length, ref status);
            if (status > 0) throw new InvalidOperationException($"ICU {status}");
            return new string(buffer, 0, Math.Min(length, buffer.Length));
        }
        finally
        {
            ureldatefmt_close(formatter);
        }
    }

    /// <summary>The languages Windows' date formatter is asked for: the one on the screen, with the system's region where it is the same language.</summary>
    private static string[] Languages()
    {
        var code = Localization.Shared.Language.Code();
        var region = System.Globalization.RegionInfo.CurrentRegion.TwoLetterISORegionName;
        return code switch
        {
            "zh-Hans" => new[] { "zh-Hans-CN" },
            "zh-Hant" => new[] { region is "HK" or "MO" ? "zh-Hant-HK" : "zh-Hant-TW" },
            "ja" => new[] { "ja-JP" },
            _ => new[] { "en-" + region, "en-US" },
        };
    }

    /// <summary>A moment as the reader's language writes it from a template, such as "month.abbreviated day hour minute".</summary>
    public static string Written(DateTimeOffset at, string template)
    {
        try
        {
            var formatter = new DateTimeFormatter(template, Languages());
            return formatter.Format(at);
        }
        catch (Exception)
        {
            return at.ToLocalTime().ToString("g", Words.Reading);
        }
    }
}
