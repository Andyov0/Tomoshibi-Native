using System.Runtime.InteropServices;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;

namespace Tomoshibi.Interop;

/// <summary>
/// A microphone through WASAPI in shared mode: 48 kHz mono floats, the
/// engine converting whatever the device gives (AUTOCONVERTPCM), drained on
/// a thread of its own as each period's packet arrives.
///
/// WASAPI rather than an audio graph, because only WASAPI says how the
/// voice is processed. A call's voice is opened as a communications stream
/// (AudioCategory_Communications), which is where Windows applies the
/// endpoint's echo canceller, noise suppression and gain control, and from
/// Windows 11 its own Voice Clarity where the driver brings none; "original
/// sound" opens it raw (AUDCLNT_STREAMOPTIONS_RAW), every effect bypassed,
/// where the microphone offers that, and as an ordinary stream where it does
/// not: Android's VOICE_COMMUNICATION, UNPROCESSED and MIC (VoiceCapture).
/// The echo canceller is told which speakers the call plays to
/// (IAcousticEchoCancellationControl, Windows 11 22H2 and later): left to
/// itself it listens to the default speakers, and a call played through
/// others came back to everybody as echo.
///
/// The system's own microphone is opened by its class rather than by a
/// device (DEVINTERFACE_AUDIO_CAPTURE), so that the stream follows the
/// system's choice of microphone by itself when a headset is plugged in.
///
/// Slots are the audioclient.h order: IAudioClient Initialize 3, Start 10,
/// Stop 11, SetEventHandle 13, GetService 14; IAudioClient2
/// SetClientProperties 16; IAudioCaptureClient GetBuffer 3, ReleaseBuffer 4,
/// GetNextPacketSize 5; IAcousticEchoCancellationControl
/// SetEchoCancellationRenderEndpoint 3; IAudioEffectsManager GetAudioEffects 5.
/// </summary>
internal sealed unsafe class WasapiCapture : IDisposable
{
    public const int Rate = 48_000;

    /// <summary>The audio capture devices' class (DEVINTERFACE_AUDIO_CAPTURE), which activates the system's microphone and follows it.</summary>
    private const string SystemMicrophone = "{2EEF81BE-33FA-4800-9670-1CD474972C3F}";

    private static readonly Guid AudioClient2Id = new("726778cd-f60a-4eda-82de-e47610cd78aa");
    private static readonly Guid CaptureClientId = new("c8adbd64-e71e-48a0-a4de-185c395cd317");
    private static readonly Guid EchoControlId = new("f4ae25b5-aaa3-437d-b6b3-dbbe2d0e9549");
    private static readonly Guid EffectsManagerId = new("4460b3ae-4b44-4527-8676-7548a8acd260");

    private const uint EventCallback = 0x00040000;
    private const uint AutoConvert = 0x80000000;
    private const uint DefaultQuality = 0x08000000;
    private const uint Silent = 0x2;
    /// <summary>AUDCLNT_E_DEVICE_INVALIDATED: the device was unplugged, turned off or taken away.</summary>
    private const int DeviceInvalidated = unchecked((int)0x88890004);

    [StructLayout(LayoutKind.Sequential)]
    private struct ClientProperties
    {
        public uint Size;
        public int IsOffload;
        public int Category;
        public uint Options;
    }

    [StructLayout(LayoutKind.Sequential, Pack = 2)]
    private struct WaveFormat
    {
        public ushort FormatTag;
        public ushort Channels;
        public uint SamplesPerSec;
        public uint AvgBytesPerSec;
        public ushort BlockAlign;
        public ushort BitsPerSample;
        public ushort Size;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct AudioEffect
    {
        public Guid Id;
        public int CanSetState;
        public int State;
    }

    private IntPtr _client;
    private IntPtr _capture;
    private readonly IntPtr _ready;
    private readonly Thread _thread;
    private volatile bool _stopped;
    private readonly Action<float[], int> _heard;

    /// <summary>The device stopped under the capture (unplugged, turned off); told once, on the capture's thread.</summary>
    public event Action? Lost;

    /// <summary>How it was opened in the end: raw falls back to plain where the microphone has no raw capture.</summary>
    public VoiceCapture.Mode Mode { get; }

    private WasapiCapture(IntPtr client, IntPtr capture, IntPtr ready, VoiceCapture.Mode mode, Action<float[], int> heard)
    {
        _client = client;
        _capture = capture;
        _ready = ready;
        _heard = heard;
        Mode = mode;
        _thread = new Thread(Run) { IsBackground = true, Name = "microphone", Priority = ThreadPriority.Highest };
    }

    /// <summary>
    /// Starts capturing <paramref name="device"/> (a device interface path,
    /// or null for the system's), as mono floats at 48 kHz handed to
    /// <paramref name="heard"/> on the capture's thread with the frame
    /// count. Throws where the device will not open.
    /// </summary>
    public static WasapiCapture Start(string? device, VoiceCapture.Mode mode, Action<float[], int> heard)
    {
        if (mode == VoiceCapture.Mode.Raw)
        {
            try
            {
                return Open(device, VoiceCapture.Mode.Raw, heard);
            }
            catch (Exception fault)
            {
                // A microphone with no raw capture refuses the option, at
                // the properties or at the start; its ordinary capture is
                // what Android takes in that case (MIC for UNPROCESSED).
                Log.Line($"no raw capture here ({fault.Message}); plain");
                mode = VoiceCapture.Mode.Plain;
            }
        }
        return Open(device, mode, heard);
    }

    private static WasapiCapture Open(string? device, VoiceCapture.Mode mode, Action<float[], int> heard)
    {
        // A client takes its properties once, before it is initialised, so
        // each try is a client of its own.
        var client = AudioActivation.Activate(device ?? SystemMicrophone, AudioClient2Id);
        IntPtr capture = IntPtr.Zero, ready = IntPtr.Zero;
        try
        {
            var properties = new ClientProperties
            {
                Size = (uint)sizeof(ClientProperties),
                IsOffload = 0,
                // AudioCategory_Communications, or AudioCategory_Other for a
                // stream with nothing of a call's processing.
                Category = mode == VoiceCapture.Mode.Communications ? 3 : 0,
                Options = mode == VoiceCapture.Mode.Raw ? 1u : 0u,
            };
            var setProperties = (delegate* unmanaged[Stdcall]<IntPtr, ClientProperties*, int>)AudioActivation.Slot(client, 16);
            Marshal.ThrowExceptionForHR(setProperties(client, &properties));
            var format = new WaveFormat
            {
                FormatTag = 3, // WAVE_FORMAT_IEEE_FLOAT
                Channels = 1,
                SamplesPerSec = Rate,
                AvgBytesPerSec = Rate * 4,
                BlockAlign = 4,
                BitsPerSample = 32,
                Size = 0,
            };
            // A tenth of a second in hand: drained every period (ten
            // milliseconds), and a voice that falls behind is worse than one
            // that drops a packet.
            var initialize = (delegate* unmanaged[Stdcall]<IntPtr, int, uint, long, long, WaveFormat*, IntPtr, int>)AudioActivation.Slot(client, 3);
            Marshal.ThrowExceptionForHR(initialize(client, 0, EventCallback | AutoConvert | DefaultQuality, 1_000_000, 0, &format, IntPtr.Zero));
            ready = AudioActivation.CreateEventW(IntPtr.Zero, 0, 0, IntPtr.Zero);
            var setEvent = (delegate* unmanaged[Stdcall]<IntPtr, IntPtr, int>)AudioActivation.Slot(client, 13);
            Marshal.ThrowExceptionForHR(setEvent(client, ready));
            capture = Service(client, CaptureClientId);
            var made = new WasapiCapture(client, capture, ready, mode, heard);
            var start = (delegate* unmanaged[Stdcall]<IntPtr, int>)AudioActivation.Slot(client, 10);
            Marshal.ThrowExceptionForHR(start(client));
            made._thread.Start();
            Log.Line($"microphone {(device == null ? "(the system's)" : "")} open, {mode.ToString().ToLowerInvariant()}: {made.Effects()}");
            return made;
        }
        catch (Exception)
        {
            if (capture != IntPtr.Zero) Marshal.Release(capture);
            if (ready != IntPtr.Zero) AudioActivation.CloseHandle(ready);
            Marshal.Release(client);
            throw;
        }
    }

    private static IntPtr Service(IntPtr client, Guid id)
    {
        IntPtr service;
        var getService = (delegate* unmanaged[Stdcall]<IntPtr, Guid*, IntPtr*, int>)AudioActivation.Slot(client, 14);
        Marshal.ThrowExceptionForHR(getService(client, &id, &service));
        return service;
    }

    /// <summary>
    /// Tells the echo canceller which speakers the call plays to: an
    /// endpoint's identifier (DeviceRules.EndpointId), or null for the
    /// system's own. False where this Windows or this microphone's echo
    /// canceller takes no such word, and it listens to the default speakers.
    /// </summary>
    public bool EchoReference(string? endpoint)
    {
        if (_client == IntPtr.Zero) return false;
        IntPtr control;
        try
        {
            control = Service(_client, EchoControlId);
        }
        catch (Exception)
        {
            return false;
        }
        try
        {
            var set = (delegate* unmanaged[Stdcall]<IntPtr, char*, int>)AudioActivation.Slot(control, 3);
            fixed (char* id = endpoint)
            {
                var result = set(control, id);
                Log.Line($"echo canceller listens to {endpoint ?? "the system's speakers"}: 0x{result:x8}");
                return result >= 0;
            }
        }
        finally
        {
            Marshal.Release(control);
        }
    }

    /// <summary>
    /// The effects the stream runs (IAudioEffectsManager, Windows 11), for
    /// the log: which of echo cancellation, noise suppression and gain
    /// control a call's voice actually has here, since a driver may bring
    /// none of them.
    /// </summary>
    public string Effects()
    {
        IntPtr manager;
        try
        {
            manager = Service(_client, EffectsManagerId);
        }
        catch (Exception)
        {
            return "effects not listed here";
        }
        try
        {
            AudioEffect* effects;
            uint count;
            var get = (delegate* unmanaged[Stdcall]<IntPtr, AudioEffect**, uint*, int>)AudioActivation.Slot(manager, 5);
            if (get(manager, &effects, &count) < 0) return "effects not listed";
            var names = new List<string>();
            for (var i = 0; i < count; i++) names.Add($"{EffectName(effects[i].Id)} {(effects[i].State != 0 ? "on" : "off")}");
            Marshal.FreeCoTaskMem((IntPtr)effects);
            return names.Count == 0 ? "no effects" : string.Join(", ", names);
        }
        finally
        {
            Marshal.Release(manager);
        }
    }

    /// <summary>The effects ksmedia.h names (AUDIO_EFFECT_TYPE_*), the rest by their GUID.</summary>
    private static string EffectName(Guid id) => id.ToString() switch
    {
        "6f64adbe-8211-11e2-8c70-2c27d7f001fa" => "echo cancellation",
        "6f64adbf-8211-11e2-8c70-2c27d7f001fa" => "noise suppression",
        "6f64adc0-8211-11e2-8c70-2c27d7f001fa" => "gain control",
        "6f64adc1-8211-11e2-8c70-2c27d7f001fa" => "beamforming",
        "6f64add0-8211-11e2-8c70-2c27d7f001fa" => "deep noise suppression",
        _ => id.ToString(),
    };

    private void Run()
    {
        var floats = new float[Rate / 10];
        var getNext = (delegate* unmanaged[Stdcall]<IntPtr, uint*, int>)AudioActivation.Slot(_capture, 5);
        var getBuffer = (delegate* unmanaged[Stdcall]<IntPtr, float**, uint*, uint*, ulong*, ulong*, int>)AudioActivation.Slot(_capture, 3);
        var release = (delegate* unmanaged[Stdcall]<IntPtr, uint, int>)AudioActivation.Slot(_capture, 4);
        while (!_stopped)
        {
            AudioActivation.WaitForSingleObject(_ready, 100);
            if (_stopped) break;
            while (true)
            {
                uint packet;
                var asked = getNext(_capture, &packet);
                if (asked == DeviceInvalidated)
                {
                    Gone();
                    return;
                }
                if (asked < 0 || packet == 0) break;
                float* data;
                uint frames, flags;
                ulong position, counter;
                var got = getBuffer(_capture, &data, &frames, &flags, &position, &counter);
                if (got == DeviceInvalidated)
                {
                    Gone();
                    return;
                }
                if (got < 0) break;
                if (floats.Length < frames) floats = new float[frames];
                if ((flags & Silent) != 0) Array.Clear(floats, 0, (int)frames);
                else new ReadOnlySpan<float>(data, (int)frames).CopyTo(floats);
                release(_capture, frames);
                try
                {
                    _heard(floats, (int)frames);
                }
                catch (Exception fault)
                {
                    Log.Line($"microphone's sound not sent: {fault.Message}");
                }
            }
        }
    }

    private void Gone()
    {
        Log.Line("the microphone went away under the capture");
        try
        {
            Lost?.Invoke();
        }
        catch (Exception fault)
        {
            Log.Line($"microphone lost: {fault.Message}");
        }
    }

    public void Dispose()
    {
        _stopped = true;
        if (_thread.IsAlive && Thread.CurrentThread != _thread) _thread.Join(500);
        if (_client != IntPtr.Zero)
        {
            var stop = (delegate* unmanaged[Stdcall]<IntPtr, int>)AudioActivation.Slot(_client, 11);
            stop(_client);
        }
        if (_capture != IntPtr.Zero) Marshal.Release(_capture);
        if (_client != IntPtr.Zero) Marshal.Release(_client);
        _capture = _client = IntPtr.Zero;
        AudioActivation.CloseHandle(_ready);
    }
}
