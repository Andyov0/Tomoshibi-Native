import CoreGraphics

/// Whether a camera's picture fills the place it is drawn in, cut at two
/// edges, or is shown whole inside it.
///
/// The web covers every camera (SurfaceTile.tsx ~60, `object-cover`), on the
/// grounds that a face is fine cropped at the edges, and the apps did the
/// same. That holds while the two shapes are close. A phone held upright in a
/// call of two, with the other person sending the landscape 4:3 of a laptop's
/// camera, kept a little over half of the picture's width, and whoever sat
/// beside them was cut out of the call: the owner's test, 2026-10-19. So the
/// share of the picture a fill keeps along the side it cuts decides: three
/// quarters or more and it fills, as before; half or more and it still fills
/// when the two are the same way round, since a tall picture on a tall phone
/// loses only the room around a face; anything less is shown whole, on a
/// blurred copy of itself rather than bars. Shared screens are not asked:
/// they are always whole, since whatever was at the edge of a screen is what
/// somebody was pointing at.
///
/// This is how the picture is drawn here and nothing anybody else in the
/// room can see, so it may differ from the web. The Android app keeps the
/// same rule and tests the same cases, so that the two never frame the same
/// picture differently; change both or neither.
public enum Framing {
    /// Whether `picture`, as it is shown (after its rotation), fills `view`.
    ///
    /// A size not yet known, or a view not yet laid out, fills: that is what
    /// every camera did before, and a first frame decided against it would
    /// otherwise jump between the two before its shape had arrived.
    public static func fills(picture: CGSize, in view: CGSize) -> Bool {
        guard picture.width > 0, picture.height > 0, view.width > 0, view.height > 0 else { return true }
        let a = picture.width / picture.height
        let b = view.width / view.height
        // The share of the picture a fill keeps along the side it cuts.
        let kept = min(a, b) / max(a, b)
        if kept >= 0.75 { return true }
        // An exact square is neither way round.
        let sameWay = (a > 1 && b > 1) || (a < 1 && b < 1)
        return sameWay && kept >= 0.5
    }
}
