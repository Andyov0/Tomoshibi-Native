using Microsoft.UI.Xaml;
using Tomoshibi.Diagnostics;
using Tomoshibi.Model;

namespace Tomoshibi;

public partial class App : Application
{
    private MainWindow? _window;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, e) => Log.Line($"unhandled: {e.Exception}");
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var launch = Launch.FromEnvironment();
        Log.Open(launch.LogFile);
        Log.Line($"Tomoshibi for Windows, core {uniffi.tomoshibi_core.TomoshibiCoreMethods.CoreVersion()}, {System.Runtime.InteropServices.RuntimeInformation.ProcessArchitecture}");
        _window = new MainWindow(launch);
        _window.Activate();
    }
}
