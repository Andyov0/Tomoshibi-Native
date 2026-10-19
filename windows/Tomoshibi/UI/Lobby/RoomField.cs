using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.UI.Kit;
using uniffi.tomoshibi_protocol;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Lobby;

/// <summary>The core's rules for room names, as the lobby asks them (Pieces.kt <c>RoomNames</c>).</summary>
internal static class RoomNames
{
    public static string Normalise(string typed)
    {
        try
        {
            return TomoshibiProtocolMethods.NormaliseRoomName(typed);
        }
        catch (Exception)
        {
            return typed.Trim().ToLowerInvariant();
        }
    }

    public static bool Valid(string name)
    {
        try
        {
            return TomoshibiProtocolMethods.ValidRoomName(name);
        }
        catch (Exception)
        {
            return false;
        }
    }

    public static string Generate() => TomoshibiProtocolMethods.GenerateRoomName();

    /// <summary>A name typed where a room goes is one only once it is something; nothing typed is no room, and asks the core nothing.</summary>
    public static bool ValidTyped(string text) => !string.IsNullOrWhiteSpace(text) && Valid(Normalise(text));
}

/// <summary>
/// Where a room's name is typed (MeetingSheet.kt <c>ScopedRoomField</c>):
/// Windows' AutoSuggestBox, which offers the account's own scopes once an
/// "@" is typed, and takes one with the arrow keys and Enter or Tab as the
/// other apps' list does; a scope this account is not in is said under it,
/// since that room will not let them in.
/// </summary>
internal sealed class RoomField : StackPanel
{
    private readonly AutoSuggestBox _box;
    private readonly TextBlock _strange;
    private readonly IReadOnlyList<string> _scopes;
    private readonly bool _admin;
    private bool _accepting;

    public event Action<string>? Changed;
    public event Action? Submitted;

    public string Text => _box.Text;

    public RoomField(string prompt, string text, IReadOnlyList<string> scopes, bool admin, bool capsule)
    {
        Spacing = 6;
        _scopes = scopes;
        _admin = admin;
        _box = new AutoSuggestBox
        {
            Text = text,
            PlaceholderText = prompt,
            QueryIcon = null,
            MinHeight = capsule ? 54 : 46,
            FontSize = capsule ? 16 : 14.5,
            UpdateTextOnSelect = false,
            VerticalContentAlignment = VerticalAlignment.Center,
        };
        _box.Named(prompt);
        _box.TextChanged += (_, e) =>
        {
            if (_accepting) return;
            if (e.Reason == AutoSuggestionBoxTextChangeReason.UserInput) Offer();
            Changed?.Invoke(_box.Text);
            Warn();
        };
        _box.SuggestionChosen += (_, e) => Accept((string)e.SelectedItem);
        _box.QuerySubmitted += (_, e) =>
        {
            if (e.ChosenSuggestion is string chosen) Accept(chosen);
            else Submitted?.Invoke();
        };
        _strange = K.Text("", 12, ink: Ink.Danger).Pad(6, 0, 6, 0);
        _strange.Arrives();
        _strange.Visibility = Visibility.Collapsed;
        Children.Add(_box);
        Children.Add(_strange);
        Loaded += (_, _) =>
        {
            // The field's own text box, so that its fill and corners are the
            // lobby's: a capsule in the join bar, a box in the sheet.
            _box.CornerRadius = new CornerRadius(capsule ? 27 : 16);
        };
    }

    public void Focus() => _box.Focus(FocusState.Programmatic);

    private (int At, string Typed) ScopePart()
    {
        var at = _box.Text.LastIndexOf('@');
        return at < 0 ? (-1, "") : (at, _box.Text[(at + 1)..].ToLowerInvariant());
    }

    private void Offer()
    {
        var (at, typed) = ScopePart();
        if (at < 0)
        {
            _box.ItemsSource = null;
            _box.IsSuggestionListOpen = false;
            return;
        }
        // An "@" typed at the end with one scope to choose takes it at once.
        if (typed.Length == 0 && _scopes.Count == 1 && _box.Text.IndexOf('@') == at && _box.Text.EndsWith('@'))
        {
            Accept(_scopes[0]);
            return;
        }
        var matches = _scopes.Where(s => s.StartsWith(typed, StringComparison.Ordinal)).ToList();
        var open = matches.Count > 0 && !(matches.Count == 1 && matches[0] == typed);
        _box.ItemsSource = open ? matches.Select(m => "@" + m).ToList() : null;
        _box.IsSuggestionListOpen = open;
    }

    private void Accept(string scope)
    {
        var (at, _) = ScopePart();
        if (at < 0) return;
        _accepting = true;
        _box.Text = _box.Text[..(at + 1)] + scope.TrimStart('@');
        _accepting = false;
        _box.IsSuggestionListOpen = false;
        Changed?.Invoke(_box.Text);
        Warn();
    }

    private void Warn()
    {
        var (at, typed) = ScopePart();
        var strange = at >= 0 && typed.Length > 0 && !_admin && !_scopes.Contains(typed) && !_box.IsSuggestionListOpen;
        if (strange) _strange.Text = T("You are not in {scope}, so this room will not let you in.", ("scope", typed));
        _strange.Visibility = strange ? Visibility.Visible : Visibility.Collapsed;
    }
}
