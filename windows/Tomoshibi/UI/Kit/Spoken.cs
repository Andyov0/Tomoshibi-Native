using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;

namespace Tomoshibi.UI.Kit;

/// <summary>
/// Something the screen says by itself, read out by a screen reader when it
/// arrives as well as shown: a polite live region (the web's notices are
/// sonner's, which are; Android announces its toasts) whose words are its
/// name, and which says that it changed once it is on the screen.
///
/// Setting the live setting alone said nothing. Narrator reads a live region
/// when the region raises LiveRegionChanged, which nothing raised, and a
/// plain panel has no automation peer to raise it from; so "somebody joined"
/// and every other notice in a call's corner went by unread. This panel has
/// a peer of its own, a group named by its words, with what it holds (a
/// notice's button, its close) inside it.
/// </summary>
internal sealed class Spoken : Grid
{
    public Spoken(UIElement content, string words)
    {
        Children.Add(content);
        AutomationProperties.SetName(this, words);
        AutomationProperties.SetLiveSetting(this, AutomationLiveSetting.Polite);
        // Each notice's words are a new one of these (the corner draws a
        // notice's line again when its words change), so arriving is the
        // one moment to say them.
        Loaded += (_, _) => Say();
    }

    private void Say()
    {
        // Nobody listening, nothing to make.
        if (!AutomationPeer.ListenerExists(AutomationEvents.LiveRegionChanged)) return;
        var peer = FrameworkElementAutomationPeer.FromElement(this) ?? FrameworkElementAutomationPeer.CreatePeerForElement(this);
        peer?.RaiseAutomationEvent(AutomationEvents.LiveRegionChanged);
    }

    protected override AutomationPeer OnCreateAutomationPeer() => new Peer(this);

    private sealed class Peer(Spoken owner) : FrameworkElementAutomationPeer(owner)
    {
        protected override AutomationControlType GetAutomationControlTypeCore() => AutomationControlType.Group;

        protected override string GetClassNameCore() => nameof(Spoken);

        protected override bool IsControlElementCore() => true;

        protected override bool IsContentElementCore() => true;
    }
}
