namespace Tomoshibi.Basics;

/// <summary>
/// Things handed over from another thread before anybody listens, kept
/// until somebody does and then given in the order they came; afterwards
/// given at once.
///
/// A link handed to the running app by a later launch (Program, the
/// lifecycle API's Activated) arrives on a thread of the system's, from the
/// moment the app has registered as the one to hand links to, which is
/// before the window, the model and the listener exist. A link clicked while
/// the app was still starting (opened by the first link, say) was handed
/// into an event nobody had subscribed to yet: dropped, the window never
/// came forward, and the second launch had already gone.
///
/// The listener is called under the handover's own lock, so that one handed
/// as the waiting ones are given cannot overtake them; it is to be quick
/// (the app's queues the work for its UI thread).
/// </summary>
public sealed class Handover<T>
{
    private readonly object _gate = new();
    private readonly List<T> _waiting = new();
    private Action<T>? _listener;

    public void Hand(T item)
    {
        lock (_gate)
        {
            if (_listener is { } listener) listener(item);
            else _waiting.Add(item);
        }
    }

    /// <summary>Listens from now on, given first whatever was handed before.</summary>
    public void Listen(Action<T> listener)
    {
        lock (_gate)
        {
            _listener = listener;
            foreach (var item in _waiting) listener(item);
            _waiting.Clear();
        }
    }
}
