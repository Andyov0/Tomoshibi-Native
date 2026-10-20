namespace Tomoshibi.Model;

/// <summary>
/// What a test asked of this launch, from the environment: the same
/// TOMOSHIBI_* switches the Apple apps read (AppModel.start) and Android
/// takes as intent extras (DebugLaunch). Nothing here is ever kept.
///
/// Only a debug build reads them: a release build is for the home
/// deployment and nothing else (the core's release profile admits no other
/// address), and it has no switches somebody could set to reach anything
/// else or to change what it captures.
///
/// A launch that names no deployment goes to the home deployment like the
/// release app, and every automatic step (autojoin, signing in, a link, a
/// screen to open on) is ignored: a test launch that forgot its base has
/// reached the production fleet before.
/// </summary>
public sealed record Launch
{
    /// <summary>The deployment to talk to (TOMOSHIBI_BASE). Null: the home deployment, and nothing automatic.</summary>
    public string? Base { get; init; }

    /// <summary>The name shown to the room (TOMOSHIBI_NAME).</summary>
    public string? Name { get; init; }

    /// <summary>Signs in as this account first (TOMOSHIBI_SIGNIN=name:file), the passphrase read from the file to keep it off the command line.</summary>
    public string? SignInName { get; init; }
    public string? SignInPassphraseFile { get; init; }

    /// <summary>A passphrase to join with (TOMOSHIBI_PASSPHRASE).</summary>
    public string? Passphrase { get; init; }

    /// <summary>Seals the call with this word (TOMOSHIBI_SEALING_WORD).</summary>
    public string? SealingWord { get; init; }

    /// <summary>Straight into this room (TOMOSHIBI_AUTOJOIN).</summary>
    public string? Autojoin { get; init; }

    /// <summary>To this room's join screen (TOMOSHIBI_PREPARE).</summary>
    public string? Prepare { get; init; }

    /// <summary>A link to open, as if the system had handed it over (TOMOSHIBI_OPEN): an invitation or an arranged meeting.</summary>
    public string? Open { get; init; }

    /// <summary>Opens on the ending screen for this reason (TOMOSHIBI_ENDED=left|closed|removed|lost), for pictures of it.</summary>
    public string? Ended { get; init; }

    /// <summary>Speaks this language for the launch without keeping it (TOMOSHIBI_LANGUAGE=en|zh-Hans|zh-Hant|ja).</summary>
    public string? Language { get; init; }

    /// <summary>
    /// Something to open over the screen (TOMOSHIBI_SHOW): settings,
    /// settings:&lt;section&gt;, start, join, schedule, servers, account, or in
    /// a call chat, devices, leave, reactions. Android's LobbyDebug.
    /// </summary>
    public string? Show { get; init; }

    /// <summary>
    /// Plays nothing and only measures what arrives (TOMOSHIBI_MUTE): a test
    /// browser's fake microphone beeps once a second, and an app in the same
    /// room would play that through somebody's real speakers.
    /// </summary>
    public bool Mute { get; init; }

    /// <summary>
    /// Opens the speakers and plays silence to them (TOMOSHIBI_MUTE=speakers):
    /// the call's audio graph runs on the device as it would, every sound in
    /// it, with the output turned to nothing, so that changing the speakers
    /// in a call can be tested without anything reaching them.
    /// </summary>
    public bool SilentSpeakers { get; init; }

    /// <summary>Joins with the microphone on (TOMOSHIBI_MIC); off otherwise, so that a test asks for nothing it did not mean to.</summary>
    public bool Microphone { get; init; }

    /// <summary>Joins with the camera on (TOMOSHIBI_CAMERA).</summary>
    public bool Camera { get; init; }

    /// <summary>
    /// Where the camera and microphone come from (TOMOSHIBI_SOURCE):
    /// <c>synthetic</c>, a generated picture and tone, or <c>devices</c>, the
    /// real ones through Windows.Media.Capture and an audio graph. Synthetic
    /// in every test: in a virtual machine the real devices may be the
    /// host's, and a test must never capture the person sitting at it.
    /// </summary>
    public bool RealDevices { get; init; } = true;

    /// <summary>
    /// Keeps what this rehearsal is told in a store of the test's own
    /// (TOMOSHIBI_KEEP=name: %LOCALAPPDATA%\Tomoshibi-tests\name), never
    /// the owner's, so that a test can sign in, quit, and see the next launch
    /// come back signed in from the sealed cookie, as a person's would.
    /// </summary>
    public string? KeepIn { get; init; }

    /// <summary>
    /// Shares straight after joining (TOMOSHIBI_SHARE=screen|sound|both):
    /// the screen, sound on its own, or both, from the generated sources
    /// unless the launch asked for the devices, and then the first screen
    /// taken without the picker, which nobody is there to answer.
    /// </summary>
    public string? Share { get; init; }

    /// <summary>
    /// Where the YouTube player's page loads YouTube's IFrame API from
    /// (TOMOSHIBI_WATCH_YOUTUBE_API): a stand-in served by the kit's own
    /// gateway, so that a test of the YouTube player never reaches YouTube.
    /// </summary>
    public string? WatchYoutubeApi { get; init; }

    /// <summary>A video to watch together straight after joining (TOMOSHIBI_WATCH), for pictures of the show.</summary>
    public string? Watch { get; init; }

    /// <summary>Whether a test asked for a share, and so takes the first screen rather than asking the picker.</summary>
    public bool SharePicked => Share != null;

    /// <summary>
    /// A picture the generated camera shows instead of its bands
    /// (TOMOSHIBI_CAMERA_PICTURE, a JPEG or PNG): a person, drifting slowly
    /// across the frame, for the background blur to find, since a test never
    /// opens a real camera.
    /// </summary>
    public string? CameraPicture { get; init; }

    /// <summary>Blurs the background from the start (TOMOSHIBI_BLUR), as the remembered choice does for somebody who turned it on.</summary>
    public bool Blur { get; init; }

    /// <summary>
    /// Runs the person segmenter on MediaPipe's test portrait at start and
    /// logs how near its mask is to MediaPipe's (TOMOSHIBI_DEBUG_SEGMENT, a
    /// folder from windows/Blur/convert.py).
    /// </summary>
    public string? SegmentCheck { get; init; }

    /// <summary>A file the measurements are written to (TOMOSHIBI_DEBUG_LOG).</summary>
    public string? LogFile { get; init; }

    /// <summary>Leaves the call and quits after this many seconds (TOMOSHIBI_DEBUG_QUIT_AFTER).</summary>
    public int? QuitAfterSeconds { get; init; }

    /// <summary>
    /// A test is driving the app (a debug build given a deployment of its
    /// own), and nothing it does is kept: not the name, the passphrase, the
    /// session, the identity, the language or the colour (Android's
    /// AppModel.rehearsal). The tests run the very app somebody uses, and
    /// what they would write over is the owner's.
    /// </summary>
    public bool Rehearsal => Base != null;

    public static Launch FromEnvironment()
    {
#if DEBUG
        static string? Get(string name)
        {
            var value = Environment.GetEnvironmentVariable(name);
            return string.IsNullOrWhiteSpace(value) ? null : value.Trim();
        }

        var baseUrl = Get("TOMOSHIBI_BASE");
        var logFile = Get("TOMOSHIBI_DEBUG_LOG");
        var quiet = Get("TOMOSHIBI_MUTE");
        // Everything below drives the app by itself, and only a launch that
        // says where to drive it to may.
        if (baseUrl == null)
        {
            return new Launch { LogFile = logFile, Mute = quiet != null && quiet != "speakers", SilentSpeakers = quiet == "speakers", SegmentCheck = Get("TOMOSHIBI_DEBUG_SEGMENT") };
        }

        string? signInName = null, signInFile = null;
        if (Get("TOMOSHIBI_SIGNIN") is { } signIn && signIn.IndexOf(':') is var colon and > 0)
        {
            signInName = signIn[..colon];
            signInFile = signIn[(colon + 1)..];
        }

        return new Launch
        {
            Base = baseUrl,
            Name = Get("TOMOSHIBI_NAME"),
            SignInName = signInName,
            SignInPassphraseFile = signInFile,
            Passphrase = Get("TOMOSHIBI_PASSPHRASE"),
            SealingWord = Get("TOMOSHIBI_SEALING_WORD"),
            Autojoin = Get("TOMOSHIBI_AUTOJOIN"),
            Prepare = Get("TOMOSHIBI_PREPARE"),
            Open = Get("TOMOSHIBI_OPEN"),
            Ended = Get("TOMOSHIBI_ENDED"),
            Language = Get("TOMOSHIBI_LANGUAGE"),
            Show = Get("TOMOSHIBI_SHOW"),
            Mute = quiet != null && quiet != "speakers",
            SilentSpeakers = quiet == "speakers",
            Microphone = Get("TOMOSHIBI_MIC") != null,
            Camera = Get("TOMOSHIBI_CAMERA") != null,
            // Synthetic unless asked: a test given a deployment of its own
            // never reaches a real device by forgetting a switch.
            RealDevices = string.Equals(Get("TOMOSHIBI_SOURCE"), "devices", StringComparison.OrdinalIgnoreCase),
            Share = Get("TOMOSHIBI_SHARE") is { } share && share is "screen" or "sound" or "both" ? share : null,
            WatchYoutubeApi = Get("TOMOSHIBI_WATCH_YOUTUBE_API"),
            Watch = Get("TOMOSHIBI_WATCH"),
            CameraPicture = Get("TOMOSHIBI_CAMERA_PICTURE"),
            Blur = Get("TOMOSHIBI_BLUR") != null,
            SegmentCheck = Get("TOMOSHIBI_DEBUG_SEGMENT"),
            KeepIn = Get("TOMOSHIBI_KEEP") is { } keep && keep.All(c => char.IsLetterOrDigit(c) || c == '-') ? keep : null,
            LogFile = logFile,
            QuitAfterSeconds = int.TryParse(Get("TOMOSHIBI_DEBUG_QUIT_AFTER"), out var seconds) && seconds > 0 ? seconds : null,
        };
#else
        return new Launch();
#endif
    }
}
