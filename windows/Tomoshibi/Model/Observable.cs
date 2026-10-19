namespace Tomoshibi.Model;

/// <summary>
/// A model the screens draw from: it says that something changed, and the
/// screens that read it bring themselves up to date (the part Compose's
/// snapshot state and SwiftUI's @Observable play on the other platforms).
/// One event for the whole model rather than one per field, because every
/// screen reads several fields and a redraw of what did not change costs
/// nothing a person could see. For the UI thread only.
/// </summary>
internal abstract class Observable
{
    public event Action? Changed;

    private int _held;
    private bool _pending;

    protected void Notify()
    {
        if (_held > 0)
        {
            _pending = true;
            return;
        }
        Changed?.Invoke();
    }

    /// <summary>Sets a field and says so when its value changed.</summary>
    protected bool Set<T>(ref T field, T value)
    {
        if (EqualityComparer<T>.Default.Equals(field, value)) return false;
        field = value;
        Notify();
        return true;
    }

    /// <summary>Several changes said once, at the end.</summary>
    protected void Batch(Action changes)
    {
        _held++;
        try
        {
            changes();
        }
        finally
        {
            _held--;
            if (_held == 0 && _pending)
            {
                _pending = false;
                Changed?.Invoke();
            }
        }
    }
}
