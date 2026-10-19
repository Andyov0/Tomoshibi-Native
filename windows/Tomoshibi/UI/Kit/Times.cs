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

    // ICU writes UTF-16: the buffer is marshalled as wide characters, where
    // the default (ANSI) gave a byte for each, and "now" came out as "no".
    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Unicode)]
    private static extern int ureldatefmt_format(IntPtr formatter, double offset, int unit, [Out] char[] result, int capacity, ref int status);

    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Unicode)]
    private static extern int ureldatefmt_formatNumeric(IntPtr formatter, double offset, int unit, [Out] char[] result, int capacity, ref int status);

    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl)]
    private static extern void ureldatefmt_close(IntPtr formatter);

    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    private static extern IntPtr udatpg_open(string locale, ref int status);

    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Unicode)]
    private static extern int udatpg_getBestPattern(IntPtr generator, string skeleton, int length, [Out] char[] pattern, int capacity, ref int status);

    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl)]
    private static extern void udatpg_close(IntPtr generator);

    // Time style first, then date style, as ICU has them; -2 is UDAT_PATTERN.
    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Unicode)]
    private static extern IntPtr udat_open(int timeStyle, int dateStyle, [MarshalAs(UnmanagedType.LPStr)] string locale, string? zone, int zoneLength, string? pattern, int patternLength, ref int status);

    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Unicode)]
    private static extern int udat_format(IntPtr format, double at, [Out] char[] result, int capacity, IntPtr position, ref int status);

    [DllImport("icu.dll", CallingConvention = CallingConvention.Cdecl)]
    private static extern void udat_close(IntPtr format);

    private const int Pattern = -2, Medium = 2, Short = 3;

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

    /// <summary>
    /// A moment written by the reader's language's best pattern for an ICU
    /// skeleton, as Android writes it (<c>DateFormat.getBestDateTimePattern</c>):
    /// "MMMdjmm" is "Oct 20, 10:00 AM" in English and "10月20日 10:00" in
    /// Chinese. Windows' own formatter, asked for the same parts, left the
    /// comma out ("Oct 20 10:00 AM").
    /// </summary>
    public static string Skeleton(DateTimeOffset at, string skeleton, string fallback)
    {
        try
        {
            var status = 0;
            var generator = udatpg_open(IcuLocale(), ref status);
            if (status > 0 || generator == IntPtr.Zero) throw new InvalidOperationException($"ICU {status}");
            string pattern;
            try
            {
                var buffer = new char[128];
                var length = udatpg_getBestPattern(generator, skeleton, skeleton.Length, buffer, buffer.Length, ref status);
                if (status > 0) throw new InvalidOperationException($"ICU {status}");
                pattern = new string(buffer, 0, Math.Min(length, buffer.Length));
            }
            finally
            {
                udatpg_close(generator);
            }
            return Format(at, Pattern, Pattern, pattern);
        }
        catch (Exception)
        {
            return Written(at, fallback);
        }
    }

    /// <summary>A moment as the reader's language writes a medium date and a short time, as Android's <c>ofLocalizedDateTime(MEDIUM, SHORT)</c>.</summary>
    public static string MediumDateShortTime(DateTimeOffset at, string fallback)
    {
        try
        {
            return Format(at, Short, Medium, null);
        }
        catch (Exception)
        {
            return Written(at, fallback);
        }
    }

    private static string Format(DateTimeOffset at, int timeStyle, int dateStyle, string? pattern)
    {
        var status = 0;
        // No zone given: ICU's default, which on Windows is the system's.
        var format = udat_open(timeStyle, dateStyle, IcuLocale(), null, 0, pattern, pattern?.Length ?? 0, ref status);
        if (status > 0 || format == IntPtr.Zero) throw new InvalidOperationException($"ICU {status}");
        try
        {
            var buffer = new char[128];
            var length = udat_format(format, at.ToUnixTimeMilliseconds(), buffer, buffer.Length, IntPtr.Zero, ref status);
            if (status > 0) throw new InvalidOperationException($"ICU {status}");
            return new string(buffer, 0, Math.Min(length, buffer.Length));
        }
        finally
        {
            udat_close(format);
        }
    }

    /// <summary>The reader's language with the system's region where it is the same language, as ICU names locales.</summary>
    private static string IcuLocale() => Languages()[0].Replace('-', '_');

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
