namespace Tomoshibi.Model;

/// <summary>
/// What a test asked of this launch, from the environment: the same
/// TOMOSHIBI_* switches the Apple apps read (AppModel.start) and Android takes
/// as intent extras (DebugLaunch). Nothing here is ever kept.
///
/// A launch that names no deployment never goes anywhere by itself. The apps
/// open on the home deployment when none is given, and a test launch that
/// forgot its base has reached it before; so autojoin, signing in and every
/// other automatic step need TOMOSHIBI_BASE, and without it the app only
/// waits for a person.
/// </summary>
public sealed record Launch
{
    /// <summary>The deployment to talk to (TOMOSHIBI_BASE). Null: the home deployment, and nothing automatic.</summary>
    public string? Base { get; init; }

    /// <summary>The name shown to the room (TOMOSHIBI_NAME).</summary>
    public string Name { get; init; } = "Native App";

    /// <summary>Signs in as this account first (TOMOSHIBI_SIGNIN=name:file), the passphrase read from the file to keep it off the command line.</summary>
    public string? SignInName { get; init; }
    public string? SignInPassphraseFile { get; init; }

    /// <summary>A passphrase to join with (TOMOSHIBI_PASSPHRASE).</summary>
    public string? Passphrase { get; init; }

    /// <summary>Seals the call with this word (TOMOSHIBI_SEALING_WORD).</summary>
    public string? SealingWord { get; init; }

    /// <summary>Straight into this room (TOMOSHIBI_AUTOJOIN).</summary>
    public string? Autojoin { get; init; }

    /// <summary>
    /// Plays nothing and only measures what arrives (TOMOSHIBI_MUTE): a test
    /// browser's fake microphone beeps once a second, and an app in the same
    /// room would play that through somebody's real speakers.
    /// </summary>
    public bool Mute { get; init; }

    /// <summary>Joins with the microphone published (TOMOSHIBI_MIC).</summary>
    public bool Microphone { get; init; }

    /// <summary>Joins with the camera published (TOMOSHIBI_CAMERA).</summary>
    public bool Camera { get; init; }

    /// <summary>
    /// Where the camera and microphone come from (TOMOSHIBI_SOURCE):
    /// <c>synthetic</c>, a generated picture and tone, or <c>devices</c>, the
    /// real ones through Windows.Media.Capture and an audio graph. Synthetic
    /// unless asked: in a virtual machine the real devices may be the host's,
    /// and a test must never capture the person sitting at it.
    /// </summary>
    public bool RealDevices { get; init; }

    /// <summary>A file the measurements are written to (TOMOSHIBI_DEBUG_LOG).</summary>
    public string? LogFile { get; init; }

    /// <summary>Leaves the call and quits after this many seconds (TOMOSHIBI_DEBUG_QUIT_AFTER).</summary>
    public int? QuitAfterSeconds { get; init; }

    public static Launch FromEnvironment()
    {
        static string? Get(string name)
        {
            var value = Environment.GetEnvironmentVariable(name);
            return string.IsNullOrWhiteSpace(value) ? null : value.Trim();
        }

        string? signInName = null, signInFile = null;
        if (Get("TOMOSHIBI_SIGNIN") is { } signIn && signIn.IndexOf(':') is var colon and > 0)
        {
            signInName = signIn[..colon];
            signInFile = signIn[(colon + 1)..];
        }

        return new Launch
        {
            Base = Get("TOMOSHIBI_BASE"),
            Name = Get("TOMOSHIBI_NAME") ?? "Native App",
            SignInName = signInName,
            SignInPassphraseFile = signInFile,
            Passphrase = Get("TOMOSHIBI_PASSPHRASE"),
            SealingWord = Get("TOMOSHIBI_SEALING_WORD"),
            Autojoin = Get("TOMOSHIBI_AUTOJOIN"),
            Mute = Get("TOMOSHIBI_MUTE") != null,
            Microphone = Get("TOMOSHIBI_MIC") != null,
            Camera = Get("TOMOSHIBI_CAMERA") != null,
            RealDevices = string.Equals(Get("TOMOSHIBI_SOURCE"), "devices", StringComparison.OrdinalIgnoreCase),
            LogFile = Get("TOMOSHIBI_DEBUG_LOG"),
            QuitAfterSeconds = int.TryParse(Get("TOMOSHIBI_DEBUG_QUIT_AFTER"), out var seconds) && seconds > 0 ? seconds : null,
        };
    }
}
