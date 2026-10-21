using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI;

/// <summary>
/// Asked before signing out (owner, 2026-10-22): Sign out signed out on the
/// press, and a press meant for the item beside it did it too. The window
/// behind goes to frosted glass, a little dimmed, so that the question is the
/// one thing in it, and the card grows in as the dialogs do (Overlay); a press
/// on the glass is Cancel. The Apple and Android apps ask the same, in the
/// same words (SignOutPrompt.swift).
/// </summary>
internal static class SignOutPrompt
{
    public static void Ask(AppModel app)
    {
        if (Overlay.Current is not { } overlay) return;
        Action close = () => { };
        var circle = new Border
        {
            Width = 46,
            Height = 46,
            CornerRadius = new CornerRadius(23),
            Background = Ink.DangerAt(0.16f),
            HorizontalAlignment = HorizontalAlignment.Center,
            Child = K.Symbol("rectangle.portrait.and.arrow.right", 17, Ink.Danger).At(HorizontalAlignment.Center, VerticalAlignment.Center),
        };
        var cancel = Tap.Words(T("Cancel"), height: 36, size: 13.5);
        cancel.HorizontalAlignment = HorizontalAlignment.Stretch;
        cancel.Click += (_, _) => close();
        var signOut = Tap.Words(T("Sign out"), look: Look.Destructive, height: 36, size: 13.5);
        signOut.HorizontalAlignment = HorizontalAlignment.Stretch;
        signOut.Named(T("Sign out"), "signout.confirm");
        signOut.Click += async (_, _) =>
        {
            close();
            await app.SignOut();
        };
        var buttons = new Grid { ColumnSpacing = 10, Margin = new Thickness(0, 4, 0, 0) };
        buttons.ColumnDefinitions.Add(new ColumnDefinition());
        buttons.ColumnDefinitions.Add(new ColumnDefinition());
        Grid.SetColumn(signOut, 1);
        buttons.Children.Add(cancel);
        buttons.Children.Add(signOut);
        var card = K.Column(14,
            circle,
            K.Column(6,
                K.Text(T("Sign out of Tomoshibi?"), 17, K.Semibold, align: TextAlignment.Center),
                K.Text(T("To come back, you will sign in again with your name and passphrase."), 13, ink: Ink.Muted, align: TextAlignment.Center)),
            buttons);
        card.Padding = new Thickness(22);
        close = overlay.ShowDialog(card, 340, null, () => close(), scrolls: false, frosted: true);
    }
}
