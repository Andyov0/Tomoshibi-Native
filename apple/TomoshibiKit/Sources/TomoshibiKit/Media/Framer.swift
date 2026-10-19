import AVFoundation
import QuartzCore
import TomoshibiBasics

/// How one remote picture sits in its place, for the view that holds it on
/// either platform (VideoSurface).
///
/// A camera fills its place or is shown whole by the two shapes (Framing),
/// decided again whenever either changes: the sender turning their phone, the
/// window being resized, a fold opening, the picture going to the stage. A
/// shared screen is always shown whole, on the tile's own ground. While a
/// camera is shown whole a blurred copy of it fills the place behind it
/// (PictureBackdrop), and only then.
///
/// At rest the picture's own gravity does the work, filling or fitting, so a
/// place that changes size or a picture that changes shape is drawn right on
/// the next frame without anything here having to keep up. The layer between
/// the view and the picture is scaled only while the framing changes: the
/// picture is fitted, and the scale goes from where it was to the share the
/// fill keeps, or back, so that it grows to fill or shrinks to be whole
/// instead of jumping between the two.
@MainActor
final class Framer {
    let renderer: VideoRenderer
    private let holder = CALayer()
    private var bounds = CGRect.zero
    private var backdrop: PictureBackdrop?
    /// The picture's shape as shown, from the renderer.
    private var picture = CGSize.zero
    /// Whether it fills now; nil before anything has been decided.
    private var fills: Bool?
    /// Whether the picture's shape was known when the framing was last
    /// decided. The first decision made with it is taken at once: a picture
    /// that arrives already shown whole should not first be seen shrinking
    /// to it.
    private var shaped = false
    /// Which change of framing is under way, so that the end of one that was
    /// overtaken does nothing.
    private var change = 0
    /// Whether the change under way has started to move the picture, or is
    /// still waiting for the backdrop's first copy.
    private var moved = true

    /// A camera, framed by its shape; otherwise always shown whole.
    var camera = false {
        didSet {
            guard camera != oldValue else { return }
            shaped = false
            decide()
        }
    }

    init(renderer: VideoRenderer) {
        self.renderer = renderer
        holder.addSublayer(renderer.layer)
        renderer.onPictureSize = { [weak self] size in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.picture = size
                    self.decide()
                }
            }
        }
    }

    /// Into the view's own layer, which clips: the picture is larger than
    /// its place for the moment it is scaled between whole and filling.
    func attach(to root: CALayer) {
        root.masksToBounds = true
        root.addSublayer(holder)
        decide()
    }

    func layout(_ bounds: CGRect) {
        self.bounds = bounds
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        holder.bounds = CGRect(origin: .zero, size: bounds.size)
        holder.position = CGPoint(x: bounds.midX, y: bounds.midY)
        renderer.layer.frame = holder.bounds
        CATransaction.commit()
        backdrop?.layout(bounds)
        decide()
    }

    /// The view is going: no more copies for a backdrop.
    func stop() {
        renderer.sample(every: 0, nil)
        backdrop?.leave(animated: false)
        backdrop = nil
    }

    private func decide() {
        let fill = camera && Framing.fills(picture: picture, in: bounds.size)
        let known = picture.width > 0 && picture.height > 0 && bounds.width > 0 && bounds.height > 0
        guard fill != fills || (known && !shaped) else { return }
        let animated = fills != nil && shaped && known
        if known { shaped = true }
        let was = fills
        fills = fill
        reframe(from: was, animated: animated)
    }

    /// The scale that makes a fitted picture fill its place: one over the
    /// share of it a fill keeps.
    private var fillScale: CGFloat {
        guard picture.width > 0, picture.height > 0, bounds.width > 0, bounds.height > 0 else { return 1 }
        let a = picture.width / picture.height, b = bounds.width / bounds.height
        return max(a, b) / min(a, b)
    }

    private func reframe(from was: Bool?, animated: Bool) {
        let fill = fills ?? true
        change += 1
        guard camera && !fill else {
            hideBackdrop(animated: animated)
            move(from: was, to: fill, animated: animated)
            return
        }
        let backdrop = showBackdrop()
        guard animated, let backdrop, !backdrop.shown else {
            move(from: was, to: fill, animated: animated)
            return
        }
        // A picture going from filling to whole waits for its blurred copy,
        // a frame or two, and shrinks onto it as it fades in. Shrunk at once,
        // it showed bars of the tile's grey around it for as long as the
        // first copy took, and the copy faded in over them after. A picture
        // that has stopped sends no frame to copy, so it waits a quarter of
        // a second at most.
        let change = self.change
        let go: @MainActor () -> Void = { [weak self] in
            guard let self, self.change == change, !self.moved else { return }
            self.move(from: was, to: fill, animated: true)
        }
        moved = false
        backdrop.onShown = go
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
            MainActor.assumeIsolated { go() }
        }
    }

    private func move(from was: Bool?, to fill: Bool, animated: Bool) {
        moved = true
        let scale = fillScale
        guard animated, scale > 1.001 else {
            rest(fill)
            return
        }
        // From wherever the picture is now, in units of the fitted picture:
        // part way through a change that this one overtakes, or filling, or
        // whole.
        let from: CGFloat
        if holder.animation(forKey: "framing") != nil, let now = holder.presentation() {
            from = now.affineTransform().a
        } else {
            from = was == true ? scale : 1
        }
        let to: CGFloat = fill ? scale : 1
        let change = self.change
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        renderer.layer.videoGravity = .resizeAspect
        holder.setAffineTransform(CGAffineTransform(scaleX: to, y: to))
        CATransaction.commit()
        CATransaction.begin()
        CATransaction.setCompletionBlock { [weak self] in
            MainActor.assumeIsolated {
                guard let self, self.change == change else { return }
                self.rest(fill)
            }
        }
        let animation = CABasicAnimation(keyPath: "transform")
        animation.fromValue = CATransform3DMakeScale(from, from, 1)
        animation.toValue = CATransform3DMakeScale(to, to, 1)
        animation.duration = Theme.settleSeconds
        animation.timingFunction = Theme.settleTiming
        holder.add(animation, forKey: "framing")
        CATransaction.commit()
    }

    /// The picture's gravity doing the work and nothing scaled, which looks
    /// the same as where a change ends.
    private func rest(_ fill: Bool) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        holder.removeAnimation(forKey: "framing")
        renderer.layer.videoGravity = fill ? .resizeAspectFill : .resizeAspect
        holder.setAffineTransform(.identity)
        CATransaction.commit()
    }

    @discardableResult
    private func showBackdrop() -> PictureBackdrop? {
        if let backdrop { return backdrop }
        guard let root = holder.superlayer else { return nil }
        let fresh = PictureBackdrop()
        root.insertSublayer(fresh.layer, below: holder)
        fresh.layout(bounds)
        backdrop = fresh
        renderer.sample(every: 1, fresh.taker())
        return fresh
    }

    private func hideBackdrop(animated: Bool) {
        guard let backdrop else { return }
        renderer.sample(every: 0, nil)
        backdrop.leave(animated: animated)
        self.backdrop = nil
    }
}
