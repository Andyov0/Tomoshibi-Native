using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Interop;
using Windows.Devices.Enumeration;
using Windows.Media;
using Windows.Media.Audio;
using Windows.Media.MediaProperties;
using Windows.Media.Render;

namespace Tomoshibi.Media;

/// <summary>
/// Plays every remote sound through one audio graph to the chosen speakers:
/// a frame input node per sound, so each keeps its own channel count and a
/// level of its own (the web's per-person volume, hearing.ts), and one per
/// pulled sound, asked for its samples a quantum at a time.
///
/// The speakers can be changed while a call plays (the devices panel): the
/// graph is made again on the new output and every sound carries on there,
/// the pushed ones from their next ten milliseconds and the pulled ones at
/// once. An audio graph's output is fixed when it is made, so the old graph
/// is stopped only once the new one runs; in between, a sound that arrives
/// goes to whichever is current.
///
/// Never made in a muted launch (TOMOSHIBI_MUTE): the graph is not even
/// opened, so a test cannot reach the speakers by any path. Made silent for
/// a test of the speakers themselves (TOMOSHIBI_MUTE=speakers): the output
/// node at nought, so the device plays digital silence.
/// </summary>
internal sealed class Playback : IDisposable
{
    /// <summary>
    /// The most a sound may queue before what arrives is dropped: a fifth of
    /// a second. Past that the voice is late enough to talk over.
    /// </summary>
    private const double MostQueuedSeconds = 0.2;

    /// <summary>The rate pulled sounds are rendered at, which their filters take every stream to.</summary>
    public const int PulledRate = 48_000;

    private sealed class Graph(AudioGraph graph, AudioDeviceOutputNode output, string? device, string name) : IDisposable
    {
        public readonly AudioGraph Audio = graph;
        public readonly AudioDeviceOutputNode Output = output;
        public readonly string? Device = device;
        public readonly string Name = name;

        /// <summary>The quanta this graph has rendered: that it is the one playing, read by the tests.</summary>
        public long Quanta;
        public readonly Dictionary<string, AudioFrameInputNode> Pushed = new();
        /// <summary>Held by the sound, not its key (PulledNodes): a stream replaced under one key fades out beside the one replacing it.</summary>
        public readonly PulledNodes<AudioFrameInputNode> Pulled = new();

        public void Dispose()
        {
            Audio.Stop();
            foreach (var node in Pushed.Values) node.Dispose();
            foreach (var node in Pulled.Nodes) node.Dispose();
            Pushed.Clear();
            Pulled.Clear();
            Audio.Dispose();
        }
    }

    private Graph _graph;
    private readonly Dictionary<string, float> _gains = new();
    /// <summary>The song desk's turning down of a sound under speech, and its fade-in, multiplied with the listener's own level (duck.ts).</summary>
    private readonly Dictionary<string, float> _ducks = new();

    private float Level(string sid) => (_gains.TryGetValue(sid, out var gain) ? gain : 1f) * (_ducks.TryGetValue(sid, out var duck) ? duck : 1f);
    private readonly object _gate = new();
    private bool _disposed;

    private readonly bool _silent;

    private Playback(Graph graph, bool silent)
    {
        _graph = graph;
        _silent = silent;
        Watch(graph);
    }

    /// <summary>
    /// The speakers stopped under the call: unplugged, turned off, or, for
    /// the system's own, the system choosing others, which a graph does not
    /// follow by itself. Told once a graph, on an audio thread; the call
    /// moves to the system's speakers (CallModel.FollowSpeakers).
    /// </summary>
    public event Action? Lost;

    private void Watch(Graph graph)
    {
        graph.Audio.UnrecoverableErrorOccurred += (_, args) =>
        {
            if (graph != _graph) return;
            Log.Line($"the speakers stopped: {args.Error}");
            Lost?.Invoke();
        };
    }

    /// <summary>The speakers it plays to: the system's own where none was picked or the one picked is gone.</summary>
    public string? Device => _graph.Device;

    public static async Task<Playback?> Open(string? device = null, bool silent = false)
    {
        var graph = await Make(device, silent) ?? (device != null ? await Make(null, silent) : null);
        return graph == null ? null : new Playback(graph, silent);
    }

    /// <summary>Where it plays and that it does, for the tests' log: the speakers' name and the quanta rendered there.</summary>
    public string Reading()
    {
        lock (_gate)
        {
            var queued = _graph.Pushed.Values.Select(n => (long)n.QueuedSampleCount).DefaultIfEmpty(0).Max();
            return $"playback{(_silent ? " silent" : "")} to {_graph.Name} quanta={Interlocked.Read(ref _graph.Quanta)} sounds={_graph.Pushed.Count + _graph.Pulled.Count} queued={queued}";
        }
    }

    private static async Task<Graph?> Make(string? device, bool silent)
    {
        var settings = new AudioGraphSettings(AudioRenderCategory.Communications);
        if (device != null)
        {
            try
            {
                settings.PrimaryRenderDevice = await DeviceInformation.CreateFromIdAsync(device);
            }
            catch (Exception fault)
            {
                Log.Line($"speakers {device} not found: {fault.Message}");
                return null;
            }
        }
        var made = await AudioGraph.CreateAsync(settings);
        if (made.Status != AudioGraphCreationStatus.Success)
        {
            Log.Line($"no audio graph for playback: {made.Status}");
            return null;
        }
        var output = await made.Graph.CreateDeviceOutputNodeAsync();
        if (output.Status != AudioDeviceNodeCreationStatus.Success)
        {
            Log.Line($"no output device for playback: {output.Status}");
            made.Graph.Dispose();
            return null;
        }
        if (silent) output.DeviceOutputNode.OutgoingGain = 0;
        var graph = new Graph(made.Graph, output.DeviceOutputNode, device, made.Graph.PrimaryRenderDevice?.Name ?? "the system's speakers");
        made.Graph.QuantumProcessed += (_, _) => Interlocked.Increment(ref graph.Quanta);
        made.Graph.Start();
        return graph;
    }

    /// <summary>
    /// Plays to these speakers from now on, the call carrying on there
    /// (null: the system's own). False where they could not be opened, and
    /// the sound stays where it was, as the web's does (preferences.ts
    /// <c>setSinkId</c> failing).
    /// </summary>
    public async Task<bool> UseOutput(string? device, bool again = false)
    {
        // Again: the same choice on a graph that stopped (Lost), which has
        // to be made anew to play at all.
        if (device == _graph.Device && !again) return true;
        var fresh = await Make(device, _silent);
        if (fresh == null) return false;
        Graph old;
        lock (_gate)
        {
            if (_disposed)
            {
                fresh.Dispose();
                return false;
            }
            old = _graph;
            _graph = fresh;
            foreach (var sound in old.Pulled.Sounds) Connect(fresh, sound);
        }
        Watch(fresh);
        // After the new graph has taken over, so that nothing played between
        // the two is lost to a graph already stopped.
        lock (_gate) old.Dispose();
        Log.Line($"playing to {(device == null ? "the system's speakers" : "the speakers picked")} ({fresh.Name})");
        return true;
    }

    /// <summary>Interleaved little-endian 16-bit samples for one sound, from the core's thread.</summary>
    public unsafe void Play(string sid, byte[] pcm, uint rate, uint channels)
    {
        AudioFrameInputNode node;
        lock (_gate)
        {
            if (_disposed) return;
            var graph = _graph;
            if (!graph.Pushed.TryGetValue(sid, out node!))
            {
                var format = AudioEncodingProperties.CreatePcm(rate, channels, 32);
                format.Subtype = MediaEncodingSubtypes.Float;
                node = graph.Audio.CreateFrameInputNode(format);
                node.OutgoingGain = Level(sid);
                node.AddOutgoingConnection(graph.Output);
                graph.Pushed[sid] = node;
            }
        }
        if (node.QueuedSampleCount > rate * MostQueuedSeconds) return;
        var samples = pcm.Length / 2;
        var frame = new AudioFrame((uint)(samples * sizeof(float)));
        using (var buffer = frame.LockBuffer(AudioBufferAccessMode.Write))
        using (var reference = buffer.CreateReference())
        {
            var (data, _) = ComCalls.Bytes(reference);
            var floats = (float*)data;
            for (var i = 0; i < samples; i++) floats[i] = (short)(pcm[2 * i] | pcm[2 * i + 1] << 8) / 32768f;
        }
        try
        {
            node.AddFrame(frame);
        }
        catch (Exception)
        {
            // The speakers changed under this sound and its node went with
            // the old graph; its next piece goes to the new ones.
        }
    }

    /// <summary>How loud one pushed sound plays, from nought to as sent; kept for a sound that arrives later.</summary>
    public void SetGain(string sid, float gain)
    {
        lock (_gate)
        {
            _gains[sid] = Math.Clamp(gain, 0f, 1f);
            if (_graph.Pushed.TryGetValue(sid, out var node)) node.OutgoingGain = Level(sid);
        }
    }

    /// <summary>How far the song desk turns one pushed sound down just now, from nought to as it is; kept for a sound that arrives later.</summary>
    public void SetDuck(string sid, float factor)
    {
        lock (_gate)
        {
            _ducks[sid] = Math.Clamp(factor, 0f, 1f);
            if (_graph.Pushed.TryGetValue(sid, out var node)) node.OutgoingGain = Level(sid);
        }
    }

    public void Forget(string sid)
    {
        lock (_gate)
        {
            _gains.Remove(sid);
            _ducks.Remove(sid);
            if (_graph.Pushed.Remove(sid, out var node)) node.Dispose();
        }
    }

    /// <summary>Starts asking <paramref name="sound"/> for its samples, on whichever speakers are current, beside any other sound under its key.</summary>
    public void Attach(PulledSound sound)
    {
        lock (_gate)
        {
            if (_disposed) return;
            Connect(_graph, sound);
        }
    }

    /// <summary>Stops asking <paramref name="sound"/>, and only it: another under its key plays on.</summary>
    public void Detach(PulledSound sound)
    {
        lock (_gate)
        {
            if (_disposed || !_graph.Pulled.Remove(sound, out var node)) return;
            node.Stop();
            node.Dispose();
        }
    }

    private static void Connect(Graph graph, PulledSound sound)
    {
        var format = AudioEncodingProperties.CreatePcm(PulledRate, 2, 32);
        format.Subtype = MediaEncodingSubtypes.Float;
        var node = graph.Audio.CreateFrameInputNode(format);
        var scratch = Array.Empty<float>();
        node.QuantumStarted += (input, asked) =>
        {
            var frames = asked.RequiredSamples;
            if (frames <= 0) return;
            if (scratch.Length < frames * 2) scratch = new float[frames * 2];
            Array.Clear(scratch, 0, frames * 2);
            sound.Render(scratch, frames);
            Hand(input, scratch, frames);
        };
        node.AddOutgoingConnection(graph.Output);
        graph.Pulled.Add(sound, node);
        node.Start();
    }

    private static unsafe void Hand(AudioFrameInputNode node, float[] stereo, int frames)
    {
        var frame = new AudioFrame((uint)(frames * 2 * sizeof(float)));
        using (var buffer = frame.LockBuffer(AudioBufferAccessMode.Write))
        using (var reference = buffer.CreateReference())
        {
            var (data, _) = ComCalls.Bytes(reference);
            var floats = (float*)data;
            for (var i = 0; i < frames * 2; i++) floats[i] = stereo[i];
        }
        node.AddFrame(frame);
    }

    public void Dispose()
    {
        lock (_gate)
        {
            _disposed = true;
            // The call's handler, which the core's lossless sink would
            // otherwise keep alive through this player after the call.
            Lost = null;
            _graph.Dispose();
        }
    }
}
