using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Screens;

/// <summary>
/// Signing in (Android's ui/screens/SignIn.kt): the account's name and its
/// passphrase, and for somebody who was sent a link instead, the way to open
/// it. Meetings here are started by the people who belong to the
/// deployment; nobody is taken into a room by its bare name.
/// </summary>
internal sealed class SignInScreen : Screen
{
    private string _name = "";
    private string _passphrase = "";
    private string _link = "";
    private bool? _wide;
    private Shared.Pressing? _go;
    private Tap? _openLink;
    private Shared.ProblemToast? _toast;
    private PasswordBox? _secret;

    public SignInScreen(AppModel app) : base(app)
    {
        SizeChanged += (_, e) => Build(Wide(e.NewSize.Width));
    }

    /// <summary>A link pasted or typed, if it is one: http, https or the app's own scheme, with a host (Pieces.kt <c>pastedLink</c>).</summary>
    public static string? PastedLink(string text)
    {
        var trimmed = text.Trim();
        if (!Uri.TryCreate(trimmed, UriKind.Absolute, out var uri)) return null;
        if (uri.Scheme is not ("https" or "http" or "tomoshibi") || string.IsNullOrEmpty(uri.Host)) return null;
        return trimmed;
    }

    private async void SignIn()
    {
        if (_name.Length == 0 || _passphrase.Length == 0 || App.Busy) return;
        await App.SignIn(_name, _passphrase);
        if (App.Account == null && _secret != null)
        {
            _secret.Password = "";
            _passphrase = "";
        }
    }

    private void OpenLink(string text)
    {
        if (PastedLink(text) is { } link) _ = App.Open(link);
    }

    private void Build(bool wide)
    {
        if (_wide == wide) return;
        _wide = wide;
        Children.Clear();

        var mark = new Mark(76);
        mark.HorizontalAlignment = HorizontalAlignment.Center;
        var heading = K.Column(12,
            mark,
            K.Text(T("Sign in"), 28, K.Bold, lines: 1, align: TextAlignment.Center),
            K.Text(T("Meetings here are started by the people who belong here. If somebody sent you a link, open that instead."), 14, ink: Ink.Muted, align: TextAlignment.Center));
        heading.Rises();

        var identity = Shared.IdentityBox(_name, text => { _name = text; Update(); }, _passphrase, text => { _passphrase = text; Update(); }, namePrompt: T("Name"), submit: SignIn);
        _secret = FindSecret(identity);
        _go = new Shared.Pressing(T("Sign in"), 48);
        _go.Named(T("Sign in"), "signin.go");
        _go.Click += (_, _) => SignIn();
        var form = K.Column(14, identity, _go);
        if (wide)
        {
            var divider = K.Line(10,
                K.Flex(new Border { Height = 1, Background = Ink.Border, VerticalAlignment = VerticalAlignment.Center }),
                K.Fixed(K.Text(T("Were you sent a link?"), 11, ink: Ink.Muted, lines: 1)),
                K.Flex(new Border { Height = 1, Background = Ink.Border, VerticalAlignment = VerticalAlignment.Center }));
            var field = Fields.Text(T("Paste the link you were sent."), _link, text => { _link = text; Update(); }, height: 38, submit: () => OpenLink(_link));
            field.FontSize = 13.5;
            field.CornerRadius = new CornerRadius(19);
            _openLink = Tap.Words(T("Join"), height: 38);
            _openLink.Click += (_, _) => OpenLink(_link);
            form.Children.Add(divider);
            form.Children.Add(K.Line(8, K.Flex(field), K.Fixed(_openLink)));
        }
        var pane = Frames.Pane(form, 30, new Thickness(18));
        pane.Rises(60);

        var column = K.Column(22, heading, pane);
        column.MaxWidth = 420;
        column.HorizontalAlignment = HorizontalAlignment.Stretch;
        var scroller = new ScrollViewer
        {
            Content = new Grid { Children = { column }, Padding = new Thickness(16, Shared.TitleBar + 56, 16, wide ? 140 : 40) },
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
        };
        var layout = new Grid();
        layout.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        layout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        layout.Children.Add(scroller);
        if (!wide)
        {
            // On a narrow window the way in for somebody with a link is at
            // the foot, where a thumb is, as the phone has it.
            var paste = new Tap(K.Row(8, K.Symbol("doc.on.clipboard", 15), K.Text(T("Paste and open"), 15, K.Medium, lines: 1)), Look.Plain, 44, padding: 20);
            paste.Resting = Tomoshibi.Basics.Theme.SurfaceHigh;
            paste.Named(T("Paste and open"));
            paste.HorizontalAlignment = HorizontalAlignment.Center;
            paste.Click += async (_, _) =>
            {
                if (await Pasteboard.Text() is { } text) OpenLink(text);
            };
            var foot = K.Column(10, K.Text(T("Were you sent a link?"), 13, ink: Ink.Muted, align: TextAlignment.Center), paste);
            foot.Margin = new Thickness(0, 0, 0, 8);
            Grid.SetRow(foot, 1);
            layout.Children.Add(foot);
        }
        _toast = new Shared.ProblemToast(App, wide ? 28 : 100);
        Children.Add(new Backdrop());
        Children.Add(layout);
        Children.Add(Shared.LanguageMenu());
        Children.Add(_toast);
        Update();
    }

    private static PasswordBox? FindSecret(DependencyObject root)
    {
        if (root is PasswordBox box) return box;
        var count = VisualTreeHelper.GetChildrenCount(root);
        if (root is Panel panel)
        {
            foreach (var child in panel.Children)
            {
                if (FindSecret(child) is { } found) return found;
            }
        }
        if (root is Border border && border.Child != null) return FindSecret(border.Child);
        return null;
    }

    public override void Update()
    {
        if (_go == null) return;
        _go.Busy = App.Busy;
        _go.IsEnabled = !App.Busy && _name.Length > 0 && _passphrase.Length > 0;
        if (_openLink != null) _openLink.IsEnabled = PastedLink(_link) != null;
        _toast?.Update();
    }
}
