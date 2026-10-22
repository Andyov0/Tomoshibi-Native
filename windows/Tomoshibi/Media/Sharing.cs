using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Model;
using uniffi.tomoshibi_core;
using uniffi.tomoshibi_protocol;

namespace Tomoshibi.Media;

/// <summary>
/// Sharing a screen, and sound on its own, each with its own tracks
/// (Android's media/Sharing.kt and the Apple apps' Sharing, whose order of
/// steps this keeps).
///
/// They are two controls and can run at once, as the web allows a screen
/// share on top of sound shared on its own (room.ts <c>share</c>). What the
/// web does about the screen's own sound then is kept: while sound is shared
/// on its own, the screen goes without its own, because the two would be one
/// song twice a beat apart; and the screen takes its sound back when the
/// sound share stops, as the Mac does.
///
/// On Windows the picture comes from the system's capture of whichever
/// screen or window the person picks in the system's picker, and the sound
/// is everything the computer plays except the call itself, through one
/// capture both may use (SoundCapture): Windows names no app's sound by the
/// window it plays in, and every app but the call is what a browser's
/// "share system audio" sends. A test shares a generated screen and a
/// generated chord, so that nothing of a real desktop or a real room is ever
/// sent.
///
/// The settings are the web's share menu (room.ts ~39-63, ShareButton.tsx):
/// a size and a frame rate, remembered between calls under the web's own
/// storage names, and the lossless switch (sound.ts <c>rememberedLossless</c>).
///
/// UI thread only.
/// </summary>
internal sealed class Sharing : Observable
{
    public const string QualityKey = "meet.share.quality";
    public const string RateKey = "meet-live.share-frame-rate";
    public const string LosslessKey = "meet-live.lossless";

    /// <summary>One setting of a screen share: a size and a rate.</summary>
    public sealed record Setting(ShareQuality Quality, int Fps);

    /// <summary>The share is not there any more: stopped while a change to it was waiting its turn or on the network.</summary>
    public sealed class Gone() : Exception("the share stopped");

    private readonly bool _synthetic;
    private readonly Func<IntPtr> _window;

    /// <summary>A debug launch's way past the picker: the first screen, taken without asking (TOMOSHIBI_SHARE=screen).</summary>
    public bool SkipPicker { get; set; }

    /// <summary>
    /// The deployment's ceiling over a share (ShareCeiling, Remote's params),
    /// read as each share starts and each change of one is made: published at
    /// one of the size's own rates no higher than the ceiling, captured no
    /// taller than its height, and its frames taken no faster than it. A
    /// ceiling changed mid-share applies from the next start or change,
    /// rather than publishing again a share somebody is watching (Android's
    /// Sharing.kt the same).
    /// </summary>
    public Func<ShareCeiling> Ceiling { get; set; } = () => ShareCeiling.None;

    private readonly Microsoft.UI.Dispatching.DispatcherQueue _ui;

    public Sharing(bool synthetic, Func<IntPtr> window)
    {
        _synthetic = synthetic;
        _window = window;
        _ui = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
    }

    /// <summary>Whether the screen is being shared.</summary>
    public bool Active { get; private set; }
    /// <summary>Whether sound is being shared on its own (the web's "listening").</summary>
    public bool Listening { get; private set; }
    /// <summary>Whether the sound shared on its own went losslessly, which its control says while it runs.</summary>
    public bool SendingLossless { get; private set; }
    /// <summary>The screen went without its sound because the sound would not start: the web's "No sound was shared".</summary>
    public bool Soundless { get; private set; }

    private int _screenWork, _soundWork;
    public bool ScreenBusy => _screenWork > 0;
    public bool SoundBusy => _soundWork > 0;

    /// <summary>
    /// Moved on by every stop of the screen and of the sound: whatever was
    /// under way before it is no longer wanted. A start part way through its
    /// publishing or a change waiting on the network sees the move when it
    /// next wakes and takes back what it made, rather than finishing into a
    /// share that was stopped or a call that was left. A stop never waits for
    /// any of them, so leaving never hangs behind the network.
    /// </summary>
    private int _screenEpoch, _soundEpoch;

    /// <summary>Changes of a running share's size, one at a time (Turns.kt).</summary>
    private readonly SemaphoreSlim _changes = new(1, 1);

    /// <summary>What the running share is sending (room.ts <c>runningShare</c>), which the menu shows while it runs.</summary>
    public Setting? Running { get; private set; }

    private IDefaults? _store;
    private bool _loading;
    private ShareQuality _quality = ShareQuality.Auto;
    private int _fps = 60;
    private bool _lossless = true;

    /// <summary>The size chosen in the menu, kept between calls.</summary>
    public ShareQuality Quality
    {
        get => _quality;
        set
        {
            _quality = value;
            if (!_loading) _store?.Set(QualityKey, ShorterSide(value).ToString());
            Notify();
        }
    }

    /// <summary>The rate chosen in the menu, kept between calls; <see cref="Rate"/> is what is sent.</summary>
    public int Fps
    {
        get => _fps;
        set
        {
            _fps = value;
            if (!_loading) _store?.Set(RateKey, value.ToString());
            Notify();
        }
    }

    /// <summary>Whether the shared thing's sound goes too. On: a share of a video with no sound is the complaint that started all of this.</summary>
    public bool WithSound { get; set; } = true;

    /// <summary>Whether sound shared alone goes losslessly as well: on unless turned off.</summary>
    public bool LosslessChosen
    {
        get => _lossless;
        set
        {
            _lossless = value;
            if (!_loading) _store?.Set(LosslessKey, value ? null : "off");
            Notify();
        }
    }

    private Call? _call;
    private VideoInput? _video;
    private AudioInput? _screenSound;
    private AudioInput? _sound;
    private ScreenSource? _screen;
    private SoundCapture? _capture;
    /// <summary>Whether the screen hears its sound, even while it has nowhere to send it (sound shared on its own has it).</summary>
    private bool _hearing;

    /// <summary>Reads the menu's choices from where this call keeps them, and keeps them there from now on.</summary>
    public void Keep(IDefaults? store)
    {
        _store = store;
        _loading = true;
        try
        {
            Quality = int.TryParse(store?.String(QualityKey), out var side) ? side switch
            {
                1080 => ShareQuality.P1080,
                1440 => ShareQuality.P1440,
                2160 => ShareQuality.P4k,
                _ => ShareQuality.Auto,
            } : ShareQuality.Auto;
            Fps = int.TryParse(store?.String(RateKey), out var rate) && rate > 0 ? rate : 60;
            LosslessChosen = store?.String(LosslessKey) != "off";
        }
        finally
        {
            _loading = false;
        }
    }

    /// <summary>The rate that goes out for a size (ShareButton.tsx <c>clamp</c>).</summary>
    public int Rate(ShareQuality quality, int? wanted = null) => ShareRates.Clamp(wanted ?? Fps, ShorterSide(quality));

    /// <summary>What the menu shows: the running share's setting while there is one, otherwise the remembered choice.</summary>
    public Setting Shown => Running ?? new Setting(Quality, Rate(Quality));

    /// <summary>The shorter side a size names, as the web's menu keys it; nought is automatic.</summary>
    public static int ShorterSide(ShareQuality quality) => quality switch
    {
        ShareQuality.P1080 => 1080,
        ShareQuality.P1440 => 1440,
        ShareQuality.P4k => 2160,
        _ => 0,
    };

    // The screen

    /// <summary>
    /// Asks the person (the system's picker) and starts sending the screen.
    /// Returns without anything shared where they cancelled, which says
    /// nothing, being an answer rather than a failure; throws where the share
    /// would not start, for the control to say so.
    /// </summary>
    public async Task Start(Call call)
    {
        if (Active || ScreenBusy) return;
        var epoch = _screenEpoch;
        _screenWork++;
        Notify();
        ScreenSource? screen = null;
        try
        {
            screen = await OpenScreen();
            if (screen == null)
            {
                Log.Line("nothing shared: nothing was picked");
                return;
            }
            if (epoch != _screenEpoch || Active)
            {
                Log.Line("a pick answered after the share was called off; nothing is shared");
                screen.Dispose();
                return;
            }
            var limits = Ceiling();
            var setting = new Setting(Quality, limits.Published(Rate(Quality), ShorterSide(Quality)));
            var fitted = ScreenSource.Fit(screen.Content, setting.Quality, setting.Fps);
            var size = limits.Size(fitted.Width, fitted.Height);
            var captured = limits.Captured(setting.Fps);
            var video = await call.PublishVideo(new VideoKind.Screen((uint)setting.Fps, setting.Quality), (uint)size.Width, (uint)size.Height);
            if (epoch != _screenEpoch)
            {
                Log.Line("the share was called off while it was published; taking it back");
                screen.Dispose();
                await Quietly(video.Unpublish);
                return;
            }
            // Its own sound unless sound is already being shared on its own,
            // which carries on and is not doubled (room.ts `alongside`).
            var hears = WithSound;
            AudioInput? sound = null;
            if (hears && TomoshibiProtocolMethods.ShareSoundAlongside(Listening))
            {
                try
                {
                    sound = await call.PublishAudio(new SoundKind.ScreenSound());
                }
                catch (Exception)
                {
                    screen.Dispose();
                    await Quietly(video.Unpublish);
                    throw;
                }
            }
            if (epoch != _screenEpoch)
            {
                Log.Line("the share was called off while its sound was published; taking it back");
                screen.Dispose();
                await Quietly(video.Unpublish);
                if (sound != null) await Quietly(sound.Unpublish);
                return;
            }
            if (hears && !Hears())
            {
                // The sound would not start: the screen goes without it, and
                // says so ("No sound was shared"), rather than not at all.
                hears = false;
                if (sound != null) await Quietly(sound.Unpublish);
                sound = null;
            }
            screen.Feed(video, captured, size);
            screen.Ended += () => Later(StopScreenFromSystem);
            if (sound != null && _capture != null)
            {
                _capture.Sink = sound;
                _capture.Lossless = false;
            }
            _call = call;
            _video = video;
            _screen = screen;
            screen = null;
            _screenSound = sound;
            _hearing = hears;
            Soundless = WithSound && !hears;
            Running = setting;
            Active = true;
            Log.Line($"sharing the screen at {ShorterSide(setting.Quality)} and {setting.Fps} fps, {size.Width}x{size.Height} captured at {captured}, with its sound {sound != null}");
            // Sound shared on its own may have started, or stopped, while
            // this was being published.
            await SoundStandsDown();
            await SoundBack();
        }
        finally
        {
            screen?.Dispose();
            _screenWork--;
            Notify();
        }
    }

    private async Task<ScreenSource?> OpenScreen()
    {
        if (_synthetic) return new SyntheticScreen();
        if (!WindowsScreen.Supported) throw new InvalidOperationException("screen capture is not supported here");
        var item = SkipPicker ? WindowsScreen.FirstScreen() : await WindowsScreen.Pick(_window());
        return item == null ? null : WindowsScreen.Start(item);
    }

    /// <summary>The capture of the computer's sound, started once for whichever share needs it first; whether it runs.</summary>
    private bool Hears()
    {
        if (_capture != null) return true;
        try
        {
            _capture = _synthetic ? new SyntheticSound() : SystemSound.Start();
            return true;
        }
        catch (Exception fault)
        {
            Log.Line($"the computer's sound would not be captured: {fault.Message}");
            return false;
        }
    }

    /// <summary>
    /// A change of size or rate from the menu: remembered, as a choice made
    /// there is, and applied at once where a share is running
    /// (ShareButton.tsx: a setting that takes effect on the next share is one
    /// nobody can judge).
    /// </summary>
    public async Task Choose(ShareQuality quality, int? fps = null)
    {
        Quality = quality;
        if (fps is { } rate) Fps = rate;
        if (!Active) return;
        try
        {
            await Retune(new Setting(quality, Rate(quality, fps ?? Shown.Fps)));
        }
        catch (Gone)
        {
            // Stopped under the change: the choice is remembered, and there is
            // nothing left to apply it to.
        }
    }

    /// <summary>
    /// The uplink card's switch: applied to the running share and not
    /// remembered (strain.ts: the next share starts from what was chosen in
    /// the menu). Throws Gone where the share stopped first, so the card does
    /// not tell the core a switch took that never happened.
    /// </summary>
    public async Task Restart(ShareQuality quality, int fps)
    {
        if (!Active) throw new Gone();
        await Retune(new Setting(quality, Rate(quality, fps)));
    }

    /// <summary>
    /// The same capture at another size and rate, with a picture track of its
    /// own (Sharing.swift <c>retune</c>): the picture is published again at
    /// the new setting and the old one taken down once the capture feeds the
    /// new one, so the share never leaves the stage for those who watch it;
    /// nothing is asked again, and the sound is untouched. One at a time, and
    /// given up where the share stops meanwhile.
    /// </summary>
    private async Task Retune(Setting asked)
    {
        await _changes.WaitAsync();
        try
        {
            var epoch = _screenEpoch;
            var (call, old, screen) = (_call, _video, _screen);
            if (!Active || call == null || old == null || screen == null) throw new Gone();
            var limits = Ceiling();
            var setting = asked with { Fps = limits.Published(asked.Fps, ShorterSide(asked.Quality)) };
            if (setting == Running) return;
            _screenWork++;
            Notify();
            try
            {
                var fitted = ScreenSource.Fit(screen.Content, setting.Quality, setting.Fps);
                var size = limits.Size(fitted.Width, fitted.Height);
                var fresh = await call.PublishVideo(new VideoKind.Screen((uint)setting.Fps, setting.Quality), (uint)size.Width, (uint)size.Height);
                if (epoch != _screenEpoch)
                {
                    // The stop took the old track down; this one is nobody's.
                    await Quietly(fresh.Unpublish);
                    throw new Gone();
                }
                screen.Feed(fresh, limits.Captured(setting.Fps), size);
                _video = fresh;
                Running = setting;
                await Quietly(old.Unpublish);
                Log.Line($"share now at {ShorterSide(setting.Quality)} and {setting.Fps} fps, {size.Width}x{size.Height}");
            }
            finally
            {
                _screenWork--;
                Notify();
            }
        }
        finally
        {
            _changes.Release();
        }
    }

    private async Task StopScreenFromSystem()
    {
        if (Active) await StopScreen();
    }

    /// <summary>
    /// Stops the screen at once, whatever is under way: what it stops is
    /// taken off the share before anything is awaited, so a change or a
    /// start that wakes after finds nothing of its own to carry on with.
    /// </summary>
    public async Task StopScreen()
    {
        _screenEpoch++;
        var (video, screenSound, screen) = (_video, _screenSound, _screen);
        _video = null;
        _screenSound = null;
        _screen = null;
        Running = null;
        Active = false;
        Soundless = false;
        _hearing = false;
        _screenWork++;
        Notify();
        try
        {
            screen?.Dispose();
            if (Listening)
            {
                if (_capture != null) _capture.Sink = _sound;
            }
            else
            {
                StopCapture();
            }
            if (video != null) await Quietly(video.Unpublish);
            if (screenSound != null) await Quietly(screenSound.Unpublish);
        }
        finally
        {
            _screenWork--;
            Notify();
        }
    }

    // Sound on its own

    /// <summary>
    /// A share of sound alone (the web's "Share only sound", sound.ts
    /// <c>startListening</c>): 510 kb/s stereo on the share's sound channel,
    /// named "listening", with no picture, and beside it the lossless stream
    /// where one is offered. Never lossless in a sealed call: the stream is
    /// data, which crosses the relay in the clear.
    /// </summary>
    public async Task StartSoundOnly(Call call, bool sealedCall)
    {
        if (Listening || SoundBusy) return;
        var epoch = _soundEpoch;
        _soundWork++;
        Notify();
        try
        {
            if (!Hears()) throw new InvalidOperationException("the computer's sound would not be captured");
            var offered = TomoshibiProtocolMethods.SoundLosslessOffered(LosslessChosen, sealedCall, true);
            var sound = await call.PublishAudio(new SoundKind.Listening(offered));
            if (epoch != _soundEpoch)
            {
                Log.Line("the sound share was called off while it was published; taking it back");
                await Quietly(sound.Unpublish);
                if (!_hearing) StopCapture();
                return;
            }
            _capture!.Sink = sound;
            _capture.Lossless = offered;
            _call ??= call;
            _sound = sound;
            Listening = true;
            SendingLossless = offered;
            Log.Line($"sharing sound, lossless offered {offered}");
            await SoundStandsDown();
        }
        finally
        {
            _soundWork--;
            Notify();
        }
    }

    public async Task StopSound()
    {
        _soundEpoch++;
        var sound = _sound;
        _sound = null;
        Listening = false;
        SendingLossless = false;
        _soundWork++;
        Notify();
        try
        {
            if (_capture != null)
            {
                _capture.Sink = null;
                _capture.Lossless = false;
            }
            if (!(Active && _hearing)) StopCapture();
            if (sound != null) await Quietly(sound.Unpublish);
            await SoundBack();
        }
        finally
        {
            _soundWork--;
            Notify();
        }
    }

    /// <summary>The screen's own sound stands down while sound shared on its own carries the sound: the two would be one song twice a beat apart.</summary>
    private async Task SoundStandsDown()
    {
        if (!Listening || _screenSound is not { } screenSound) return;
        if (_capture != null)
        {
            _capture.Sink = _sound;
            _capture.Lossless = SendingLossless;
        }
        _screenSound = null;
        await Quietly(screenSound.Unpublish);
    }

    /// <summary>
    /// And the screen, still shared, has its sound back once sound is no
    /// longer shared on its own. Whatever changed while it was published (the
    /// screen stopped, sound shared again) takes it straight back down.
    /// </summary>
    private async Task SoundBack()
    {
        if (!Active || !_hearing || Listening || _screenSound != null || _call is not { } call) return;
        var epoch = _screenEpoch;
        AudioInput back;
        try
        {
            back = await call.PublishAudio(new SoundKind.ScreenSound());
        }
        catch (Exception)
        {
            return;
        }
        if (epoch != _screenEpoch || !Active || Listening || _screenSound != null)
        {
            await Quietly(back.Unpublish);
            return;
        }
        if (!Hears())
        {
            await Quietly(back.Unpublish);
            return;
        }
        _screenSound = back;
        _capture!.Sink = back;
        _capture.Lossless = false;
    }

    private void StopCapture()
    {
        _capture?.Dispose();
        _capture = null;
    }

    /// <summary>Everything stopped: leaving, or the call ending under it.</summary>
    public async Task Stop()
    {
        // The screen first, so that stopping the sound does not hand the
        // screen its sound back on the way out.
        await StopScreen();
        await StopSound();
        _call = null;
    }

    /// <summary>What the share is doing, for a debug log.</summary>
    public string State =>
        $"screen {(Active ? "on" : "off")} busy {ScreenBusy} running {(Running is { } r ? $"{ShorterSide(r.Quality)}@{r.Fps}" : "-")} " +
        $"sent {_screen?.FramesPushed ?? 0}, sound {(Listening ? "on" : "off")} busy {SoundBusy} lossless {SendingLossless} chunks {_capture?.ChunksPushed ?? 0}";

    /// <summary>Starts work on the UI thread, from a callback of the system's that cannot wait for it.</summary>
    private void Later(Func<Task> work) => _ui.TryEnqueue(() => _ = Quietly(work));

    private static async Task Quietly(Func<Task> work)
    {
        try
        {
            await work();
        }
        catch (Exception fault)
        {
            Log.Line($"ignored: {fault.Message}");
        }
    }
}
