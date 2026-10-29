namespace Tomoshibi.Basics;

/// <summary>
/// When the call's controls step aside (ControlBar.tsx <c>away</c>, with
/// hooks/useIdle.ts deciding <paramref name="quietSeconds"/>): only where they
/// may (the "hide when nothing is happening" placement, or a stage filling the
/// screen), only once nobody has moved, pressed a key or touched anything for
/// <see cref="Plan.IdleAfter"/> seconds, and never while something of the
/// call's chrome is in use: a menu or a panel open, or the keyboard's focus
/// on one of the controls themselves, which the web keeps where they are
/// for as long as a focus it shows (<c>:focus-visible</c>) is inside them.
///
/// Nor while a screen reader is listening. Somebody using Narrator has no
/// pointer to bring the controls back with, and the controls are how they
/// mute and leave; on Windows they were collapsed after three seconds and
/// could be brought back only by the mouse, so a keyboard or Narrator user
/// in a filled stage or with the idle placement could not mute, turn the
/// camera off or leave. Stepped aside, they are faded and moved down but
/// left in place, as the web leaves them, so that the keyboard can still
/// reach them and reaching them brings them back.
/// </summary>
public static class ControlsAway
{
    public static bool Away(
        bool mayStepAside,
        double quietSeconds,
        bool menuOpen,
        bool panelOpen,
        bool focusInside,
        bool readerListening) =>
        mayStepAside && quietSeconds > Plan.IdleAfter && !menuOpen && !panelOpen && !focusInside && !readerListening;
}

/// <summary>
/// The call's two shortcuts, the Mac's Call menu's (⌘D the microphone, ⌘E the
/// camera; apple/App/TomoshibiApp.swift) with Control for Command, the
/// way Windows writes a Mac shortcut. On Windows they are keyboard accelerators of the
/// bar's own two buttons, so they do what a press does, are named to
/// Narrator with the button, and work wherever the call has the keyboard,
/// while the bar is stepped aside as well.
/// </summary>
public static class CallKeys
{
    /// <summary>The letter that, with Control, mutes or unmutes the microphone.</summary>
    public const char Microphone = 'D';

    /// <summary>The letter that, with Control, turns the camera off or on.</summary>
    public const char Camera = 'E';
}
