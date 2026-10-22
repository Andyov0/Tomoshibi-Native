#if os(iOS)
import LinkPresentation
import SwiftUI
import TomoshibiBasics
import UIKit

/// A meeting's or an invitation's link handed to the system's share sheet,
/// with what Messages should draw it as.
///
/// The app had no way to send a link but to copy it. Handed over bare, a
/// link's preview is whatever the system makes of the page, and the
/// deployment's page says nothing but its title: the share sheet read it as
/// "Tomoshibi" over the host, with no picture, and Messages draws and sends
/// the preview the same way. Handed LPLinkMetadata with the link, the sheet
/// and Messages draw the invitation's sentence and the mark instead, the
/// same picture the Messages extension's card carries (InvitePicture),
/// whatever the page says. The sentence is the one the web's page for an
/// opened invitation greets a guest with (routes/Landing.tsx).
@MainActor
enum LinkShare {
    static func present(_ url: URL, room: String, from anchor: UIView?) {
        guard let anchor, let window = anchor.window, var top = window.rootViewController else { return }
        while let shown = top.presentedViewController, !shown.isBeingDismissed { top = shown }
        let item = LinkItem(url: url, room: room, metadata: metadata(url, room: room))
        let sheet = UIActivityViewController(activityItems: [item], applicationActivities: nil)
        // An iPad shows the sheet as a popover, which needs somewhere to hang
        // from, or presenting it throws.
        sheet.popoverPresentationController?.sourceView = anchor
        sheet.popoverPresentationController?.sourceRect = anchor.bounds
        top.present(sheet, animated: true)
    }

    /// What Messages draws the link as.
    static func metadata(_ url: URL, room: String) -> LPLinkMetadata {
        let metadata = LPLinkMetadata()
        metadata.originalURL = url
        metadata.url = url
        metadata.title = t("You were invited to {room}.", ["room": room])
        if let picture = InvitePicture.image() { metadata.imageProvider = NSItemProvider(object: picture) }
        if let icon = InvitePicture.icon() { metadata.iconProvider = NSItemProvider(object: icon) }
        return metadata
    }
}

/// The link, and its preview, for whichever activity is chosen: the address
/// itself to everything, the sentence as a subject to mail, and the drawn
/// preview to whatever asks for one.
private final class LinkItem: NSObject, UIActivityItemSource {
    let url: URL
    let room: String
    let metadata: LPLinkMetadata

    init(url: URL, room: String, metadata: LPLinkMetadata) {
        self.url = url
        self.room = room
        self.metadata = metadata
    }

    func activityViewControllerPlaceholderItem(_ controller: UIActivityViewController) -> Any { url }

    func activityViewController(_ controller: UIActivityViewController, itemForActivityType type: UIActivity.ActivityType?) -> Any? { url }

    func activityViewController(_ controller: UIActivityViewController, subjectForActivityType type: UIActivity.ActivityType?) -> String {
        t("You were invited to {room}.", ["room": room])
    }

    func activityViewControllerLinkMetadata(_ controller: UIActivityViewController) -> LPLinkMetadata? { metadata }
}

/// A button that hands a link to the share sheet, hung from the button
/// itself. The link is asked for when it is pressed, so that an invitation
/// can be made for the press rather than ahead of it.
struct LinkShareButton<Label: View>: View {
    let room: String
    let link: () async -> URL?
    @ViewBuilder let label: () -> Label
    @State private var anchor = ShareAnchor()

    var body: some View {
        Button {
            Task { @MainActor in
                guard let url = await link() else { return }
                LinkShare.present(url, room: room, from: anchor.view)
            }
        } label: {
            label()
        }
        .background(ShareAnchorView(anchor: anchor))
    }
}

/// A view to hang the share sheet from: the button it is put behind.
final class ShareAnchor {
    weak var view: UIView?
}

struct ShareAnchorView: UIViewRepresentable {
    let anchor: ShareAnchor

    func makeUIView(context: Context) -> UIView {
        let view = UIView()
        view.isUserInteractionEnabled = false
        view.backgroundColor = .clear
        anchor.view = view
        return view
    }

    func updateUIView(_ view: UIView, context: Context) {
        anchor.view = view
    }
}
#endif
