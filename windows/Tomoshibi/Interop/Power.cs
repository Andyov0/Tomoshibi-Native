using System.Runtime.InteropServices;

namespace Tomoshibi.Interop;

/// <summary>
/// The thread's execution state (Tomoshibi.Basics.KeepAwake says which and
/// when): kept on the window's thread, which lives as long as the app, since
/// the state belongs to the thread that asked and goes when it ends.
/// </summary>
internal static class Power
{
    [DllImport("kernel32.dll")]
    private static extern uint SetThreadExecutionState(uint flags);

    /// <summary>Asks for <paramref name="flags"/>; what was in effect before, or nought where it was refused.</summary>
    public static uint Ask(uint flags) => SetThreadExecutionState(flags);
}
