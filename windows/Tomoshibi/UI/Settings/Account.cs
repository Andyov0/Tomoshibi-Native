using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Model;
using Tomoshibi.UI.Kit;
using Tomoshibi.UI.Lobby;
using uniffi.tomoshibi_protocol;
using Windows.Graphics.Imaging;
using Windows.Storage.Pickers;
using Windows.Storage.Streams;
using static Tomoshibi.Basics.Words;

namespace Tomoshibi.UI.Settings;

/// <summary>
/// The account's picture, chosen from the person's files (Platform.kt
/// <c>PictureShrinker</c>): the middle square of it, 256 pixels across, on
/// black where it is transparent, as a JPEG at 82, which is what every
/// platform sends so that a picture looks the same whoever set it.
/// </summary>
internal static class Picture
{
    public static async Task<string?> Choose()
    {
        var picker = new FileOpenPicker { ViewMode = PickerViewMode.Thumbnail, SuggestedStartLocation = PickerLocationId.PicturesLibrary };
        foreach (var type in new[] { ".jpg", ".jpeg", ".png", ".heic", ".webp", ".bmp", ".gif" }) picker.FileTypeFilter.Add(type);
        // An unpackaged app's picker needs to be told whose window it is over.
        if (MainWindow.Current is { } window) WinRT.Interop.InitializeWithWindow.Initialize(picker, WinRT.Interop.WindowNative.GetWindowHandle(window));
        var file = await picker.PickSingleFileAsync();
        if (file == null) return null;
        try
        {
            using var stream = await file.OpenReadAsync();
            var decoder = await BitmapDecoder.CreateAsync(stream);
            var side = Math.Min(decoder.OrientedPixelWidth, decoder.OrientedPixelHeight);
            var scale = 256.0 / side;
            var transform = new BitmapTransform
            {
                ScaledWidth = (uint)Math.Round(decoder.OrientedPixelWidth * scale),
                ScaledHeight = (uint)Math.Round(decoder.OrientedPixelHeight * scale),
                InterpolationMode = BitmapInterpolationMode.Fant,
            };
            transform.Bounds = new BitmapBounds
            {
                X = (transform.ScaledWidth - 256) / 2,
                Y = (transform.ScaledHeight - 256) / 2,
                Width = 256,
                Height = 256,
            };
            var pixels = await decoder.GetPixelDataAsync(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied, transform, ExifOrientationMode.RespectExifOrientation, ColorManagementMode.ColorManageToSRgb);
            var bytes = pixels.DetachPixelData();
            // On black where it was transparent: premultiplied, the colour is already on black.
            for (var i = 3; i < bytes.Length; i += 4) bytes[i] = 255;
            using var output = new InMemoryRandomAccessStream();
            var options = new BitmapPropertySet { ["ImageQuality"] = new BitmapTypedValue(0.82, Windows.Foundation.PropertyType.Single) };
            var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.JpegEncoderId, output, options);
            encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Ignore, 256, 256, 96, 96, bytes);
            await encoder.FlushAsync();
            var jpeg = new byte[output.Size];
            output.Seek(0);
            using var reader = new DataReader(output.GetInputStreamAt(0));
            await reader.LoadAsync((uint)output.Size);
            reader.ReadBytes(jpeg);
            return "data:image/jpeg;base64," + Convert.ToBase64String(jpeg);
        }
        catch (Exception fault)
        {
            Log.Line($"the picture could not be read: {fault.GetType().Name}");
            return null;
        }
    }
}

/// <summary>
/// Guest links (settings/GuestLinks.kt; Account.tsx <c>GuestLinks</c>): a
/// standing link into a room of one of your scopes, for somebody outside it,
/// with a window it works in if you want one, and the way to stop every
/// link into a room. The window's ends are Windows' own date and time
/// pickers.
/// </summary>
internal static class GuestLinks
{
    public static StackPanel Section(AppModel app, Account account)
    {
        string room = "", name = "";
        string scope = account.Scopes.FirstOrDefault() ?? "";
        DateTimeOffset? from = null, until = null;
        string? busy = null;

        var made = K.Column(8);
        made.Visibility = Visibility.Collapsed;
        made.Arrives();
        var result = K.Text("", 12);
        result.Visibility = Visibility.Collapsed;
        result.Arrives();

        string Full() => RoomNames.Normalise(room.Replace("@", "-")) + "@" + scope;
        bool Usable()
        {
            try
            {
                return RoomNames.Valid(Full()) && TomoshibiProtocolMethods.ValidScope(scope);
            }
            catch (Exception)
            {
                return false;
            }
        }

        var make = Tap.Words(T("Make a link"), "link", Look.Prominent, 34);
        var stop = Tap.Words(T("Stop every link to this room working"), "xmark.circle", Look.Plain, 34, 13);
        void Ready()
        {
            make.IsEnabled = busy == null && Usable();
            stop.IsEnabled = busy == null && Usable();
        }

        var roomBox = Fields.Text(T("Room name"), "", text => { room = text; Ready(); }, mono: true);
        UIElement scopeControl;
        if (account.Admin)
        {
            var box = Fields.Text(T("Scope"), scope, text => { scope = text.Trim().ToLowerInvariant(); Ready(); }, mono: true);
            box.Width = 140;
            scopeControl = box;
        }
        else if (account.Scopes.Length > 1)
        {
            var combo = new ComboBox { MinWidth = 120 };
            foreach (var each in account.Scopes) combo.Items.Add(each);
            combo.SelectedIndex = 0;
            combo.SelectionChanged += (_, _) => { scope = (string)combo.SelectedItem; Ready(); };
            scopeControl = combo;
        }
        else
        {
            scopeControl = K.Text(scope, 14, mono: true).At(HorizontalAlignment.Left, VerticalAlignment.Center);
        }
        var roomLine = K.Line(8, K.Flex(roomBox), K.Fixed(K.Text("@", 15, mono: true, ink: Ink.Muted).At(HorizontalAlignment.Center, VerticalAlignment.Center)), K.Fixed(scopeControl));
        var nameBox = Fields.Text(T("Their name, if the link is for one person"), "", text => name = text);

        StackPanel Moment(string label, Action<DateTimeOffset?> set)
        {
            var date = new CalendarDatePicker { PlaceholderText = "—", MinDate = DateTimeOffset.Now.Date, Language = Localization.Shared.Language.Code() };
            var time = new TimePicker { ClockIdentifier = "24HourClock", MinuteIncrement = 15, Language = Localization.Shared.Language.Code(), IsEnabled = false };
            var next = DateTimeOffset.Now.AddHours(1);
            time.Time = new TimeSpan(next.Hour, 0, 0);
            void Tell()
            {
                if (date.Date is not { } day)
                {
                    set(null);
                    time.IsEnabled = false;
                    return;
                }
                time.IsEnabled = true;
                set(new DateTimeOffset(day.Year, day.Month, day.Day, 0, 0, 0, DateTimeOffset.Now.Offset).Add(time.Time));
            }
            date.DateChanged += (_, _) => Tell();
            time.TimeChanged += (_, _) => Tell();
            date.Named(label);
            return K.Column(5, K.Text(label, 11, ink: Ink.Muted), K.Row(8, date, time));
        }
        var window = new VariableSizedWrapGrid { Orientation = Orientation.Horizontal };
        var fromPicker = Moment(T("Works from"), at => from = at);
        fromPicker.Margin = new Thickness(0, 0, 12, 10);
        window.Children.Add(fromPicker);
        window.Children.Add(Moment(T("Works until"), at => until = at));

        make.Click += async (_, _) =>
        {
            busy = "make";
            Ready();
            result.Visibility = Visibility.Collapsed;
            var (link, refusal) = await app.MakeGuestLink(Full(), name, from, until);
            busy = null;
            Ready();
            if (link != null)
            {
                Pasteboard.Copy(link.Link);
                made.Children.Clear();
                var text = K.Text(link.Link, 11.5, mono: true, ink: Ink.ForegroundAt(0.85f));
                text.IsTextSelectionEnabled = true;
                var box = new Border { Child = text, Background = Ink.Well(0.25f), CornerRadius = new CornerRadius(11), Padding = new Thickness(10) };
                made.Children.Add(K.Line(8, K.Flex(box), K.Fixed(Screens.PreJoinScreen.CopyLink(link.Link))));
                made.Children.Add(K.Text(Describe(link), 11.5, ink: Ink.Muted));
                made.Visibility = Visibility.Visible;
            }
            else
            {
                result.Text = refusal ?? "";
                result.Foreground = Ink.DangerText;
                result.Visibility = Visibility.Visible;
            }
        };
        stop.Click += async (_, _) =>
        {
            busy = "stop";
            Ready();
            var (count, refusal) = await app.StopGuestLinks(Full());
            busy = null;
            Ready();
            if (count is { } stopped)
            {
                made.Visibility = Visibility.Collapsed;
                result.Text = stopped == 1 ? T("One link stopped working.") : T("{count} links stopped working.", ("count", stopped));
                result.Foreground = Ink.GoodText;
            }
            else
            {
                result.Text = refusal ?? "";
                result.Foreground = Ink.DangerText;
            }
            result.Visibility = Visibility.Visible;
        };
        Ready();
        var column = K.Flowing(12);
        column.Children.Add(roomLine);
        column.Children.Add(nameBox);
        column.Children.Add(window);
        column.Children.Add(K.Row(8, make, stop));
        column.Children.Add(made);
        column.Children.Add(result);
        return Rows.Section(T("Guest links"), K.Card(column, 16, padding: new Thickness(14)),
            T("A link lets somebody outside your scope into one of its rooms as a guest. Leave the end empty for a link that works until it is stopped."));
    }

    /// <summary>When the link works and for whom (GuestLinks.kt <c>describe</c>).</summary>
    private static string Describe(MadeInvite made)
    {
        var from = Refusals.Instant(made.From);
        var until = Refusals.Instant(made.Until);
        var said = (from, until) switch
        {
            ({ } f, { } u) => T("This link works from {from} until {until}.", ("from", LobbyModel.Said(f)), ("until", LobbyModel.Said(u))),
            (null, { } u) => T("This link works until {until}.", ("until", LobbyModel.Said(u))),
            ({ } f, null) => T("This link works from {from} until somebody stops it.", ("from", LobbyModel.Said(f))),
            _ => T("This link works until somebody stops it."),
        };
        if (!string.IsNullOrEmpty(made.Name)) said += " " + T("Whoever uses it joins as {name}.", ("name", made.Name));
        return said;
    }
}
