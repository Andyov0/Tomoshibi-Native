import Messages
import SwiftUI
import TomoshibiBasics
import UIKit

/// Tomoshibi in Messages: a meeting started for now, or one already arranged,
/// sent into the conversation as a card, and a card that was sent opened into
/// the meeting.
///
/// The card's link is the meeting's own link as the web hands it out
/// (MeetingLink). With the app, a press on the card opens this extension over
/// the conversation, and Join hands the link to the app under its own scheme,
/// which is how the web's page asks the app to open a link (landing.ts
/// `appLink`). Without it, the same link goes beside the card as text
/// (InviteModel `send`), and opens in the browser, where the deployment's
/// page offers the app or the browser.
///
/// An extension, and so linked with TomoshibiBasics alone: Messages gives one
/// a small part of the memory the app has, and the core, with WebRTC inside
/// it, is more than all of it. What it knows of the account comes from the
/// app (Handover), and what it asks of the deployment it asks itself
/// (MeetingsAPI).
final class MessagesViewController: MSMessagesAppViewController {
    private lazy var model = InviteModel(
        insert: { [weak self] message in await self?.insert(message) ?? false },
        insertText: { [weak self] text in await self?.insert(text) ?? false },
        open: { [weak self] url in self?.extensionContext?.open(url) },
        collapse: { [weak self] in
            guard let self, self.presentationStyle == .expanded else { return }
            self.requestPresentationStyle(.compact)
        })

    override func viewDidLoad() {
        super.viewDidLoad()
        // The ground under SwiftUI, so that the moment before it draws, and
        // the edge Messages animates when the view grows, are the room's
        // ground rather than the system's, in either appearance.
        view.backgroundColor = Theme.backgroundUIColor
        let hosting = UIHostingController(rootView: InviteRoot(model: model))
        hosting.view.backgroundColor = .clear
        addChild(hosting)
        hosting.view.frame = view.bounds
        hosting.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(hosting.view)
        hosting.didMove(toParent: self)
    }

    override func willBecomeActive(with conversation: MSConversation) {
        super.willBecomeActive(with: conversation)
        model.style = presentationStyle
        model.show(conversation.selectedMessage)
        Task { await model.refresh() }
    }

    /// A card pressed while the extension is already open.
    override func didSelect(_ message: MSMessage, conversation: MSConversation) {
        super.didSelect(message, conversation: conversation)
        model.show(message)
    }

    override func willTransition(to presentationStyle: MSMessagesAppPresentationStyle) {
        super.willTransition(to: presentationStyle)
        model.style = presentationStyle
    }

    /// Puts the card in the field the person sends from, beside whatever they
    /// typed, rather than sending it: they may want to say something with it,
    /// and a card that went at a press with nothing to say it was coming is
    /// not what anybody expects of a button in Messages.
    private func insert(_ message: MSMessage) async -> Bool {
        guard let conversation = activeConversation else { return false }
        do {
            try await conversation.insert(message)
            return true
        } catch {
            return false
        }
    }

    private func insert(_ text: String) async -> Bool {
        guard let conversation = activeConversation else { return false }
        do {
            try await conversation.insertText(text)
            return true
        } catch {
            return false
        }
    }
}
