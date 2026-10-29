namespace Tomoshibi.Basics;

/// <summary>
/// The display and the machine kept awake while a call is on, and let go
/// when it ends (SetThreadExecutionState's flags; what a browser does with a
/// wake lock while a call plays). Without it Windows turned the display off
/// and then slept in the middle of a call somebody was only watching or
/// listening to: a shared screen went dark under them, and sleep dropped the
/// call itself.
///
/// The flags are given to <c>set</c>, which on Windows is
/// SetThreadExecutionState on the window's thread: the state belongs to the
/// thread that asked for it and lasts until it asks again or ends, so it is
/// asked on the thread that lives as long as the app. Asked only when the
/// wish changes, so a call that is joined again (moved to another relay)
/// does not let go and take hold again on its way.
/// </summary>
public sealed class KeepAwake
{
    /// <summary>ES_CONTINUOUS: what is asked lasts until it is asked again.</summary>
    public const uint Continuous = 0x80000000;

    /// <summary>ES_SYSTEM_REQUIRED: the machine does not sleep.</summary>
    public const uint SystemRequired = 0x00000001;

    /// <summary>ES_DISPLAY_REQUIRED: the display stays on.</summary>
    public const uint DisplayRequired = 0x00000002;

    /// <summary>What a call asks for.</summary>
    public const uint InCall = Continuous | DisplayRequired | SystemRequired;

    /// <summary>What leaving it asks for: continuous and nothing else, which clears what the call asked.</summary>
    public const uint Released = Continuous;

    private readonly Func<uint, uint> _set;

    /// <param name="set">Asks for these flags and says what was in effect before, nought where it was refused.</param>
    public KeepAwake(Func<uint, uint> set) => _set = set;

    /// <summary>Whether the call's wish is in effect.</summary>
    public bool Held { get; private set; }

    /// <summary>
    /// Holds the display and the machine awake, or lets them go, when that
    /// changes. Whether the system took the request: refused, what was in
    /// effect stays as it was, and the same wish asks again.
    /// </summary>
    public bool Hold(bool wanted)
    {
        if (wanted == Held) return true;
        var took = _set(wanted ? InCall : Released) != 0;
        if (took) Held = wanted;
        return took;
    }
}
