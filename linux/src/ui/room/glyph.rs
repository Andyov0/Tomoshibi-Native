//! The few symbols the call needs that the system's icon theme has no
//! version of: a raised hand, a crown, a filled message bubble, the
//! clapperboard of watching together and the send arrow. Everything else on the bar is Adwaita's own symbolic icon,
//! so that a control means on Linux what it looks like on Linux, the rule
//! windows/Tomoshibi/UI/Kit/Symbols.cs follows with Segoe; where the set
//! has nothing near (a hand, a crown) the symbol is drawn as a path, in the
//! widget's own colour as a symbolic icon is, with the same paths as
//! Windows and Android draw.

use std::cell::{Cell, RefCell};

use gtk::{glib, graphene, gsk, prelude::*, subclass::prelude::*};

/// One symbol: SVG path data on a square grid, filled or stroked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub data: &'static str,
    /// The grid the path is drawn on.
    pub grid: f32,
    /// A stroke this wide, in grid units, rather than a fill.
    pub stroke: Option<f32>,
}

/// Material's rounded back_hand, the raised hand Android draws and Windows
/// copies (Symbols.cs `Hand`), on its 24-unit grid.
pub const HAND: Shape = Shape {
    data: "M21,16 c0,4.42 -3.58,8 -8,8 c-3.26,0 -6.19,-1.99 -7.4,-5.02 l-3.03,-7.61 C2.26,10.58 3,9.79 3.81,10.05 l0.79,0.26 c0.56,0.18 1.02,0.61 1.24,1.16 l1.28,3.21 C7.2,14.88 7.38,15 7.58,15 H8 V3.25 C8,2.56 8.56,2 9.25,2 s1.25,0.56 1.25,1.25 v8.25 c0,0.28 0.22,0.5 0.5,0.5 c0.28,0 0.5,-0.22 0.5,-0.5 V1.25 C11.5,0.56 12.06,0 12.75,0 S14,0.56 14,1.25 V11.5 c0,0.28 0.22,0.5 0.5,0.5 c0.28,0 0.5,-0.22 0.5,-0.5 V2.75 c0,-0.69 0.56,-1.25 1.25,-1.25 c0.69,0 1.25,0.56 1.25,1.25 v8.75 c0,0.28 0.22,0.5 0.5,0.5 c0.28,0 0.5,-0.22 0.5,-0.5 V5.75 c0,-0.69 0.56,-1.25 1.25,-1.25 S21,5.06 21,5.75 V16 Z",
    grid: 24.0,
    stroke: None,
};

/// The crown of whoever runs the room (Symbols.cs), on a 16-unit grid.
pub const CROWN: Shape = Shape {
    data:
        "M 1,10 L 2,4 L 5,7 L 8,2 L 11,7 L 14,4 L 15,10 Z M 2,11.5 L 14,11.5 L 14,13.5 L 2,13.5 Z",
    grid: 16.0,
    stroke: None,
};

/// A message bubble, filled as the Mac's `bubble.left.fill` is; the web's
/// message-square outline, closed and filled.
pub const MESSAGE: Shape = Shape {
    data: "M 5,3 H 19 C 20.1,3 21,3.9 21,5 V 15 C 21,16.1 20.1,17 19,17 H 7.5 L 3.6,20.6 C 3.3,20.9 3,20.7 3,20.3 V 5 C 3,3.9 3.9,3 5,3 Z",
    grid: 24.0,
    stroke: None,
};

/// Watching together's clapperboard (the web's Clapperboard, ControlBar.tsx),
/// stroked on its 24-unit grid: the icon theme's film symbols are a reel and
/// a strip of film, which read as a recording.
pub const CLAPPERBOARD: Shape = Shape {
    data: "M 20.2,6 L 3,11 L 2.1,8.6 C 1.8,7.5 2.4,6.4 3.4,6.1 L 16.9,2.1 C 18,1.8 19.1,2.4 19.4,3.4 Z M 6.2,5.3 L 9.3,9.2 M 12.4,3.4 L 15.5,7.4 M 3,11 L 21,11 L 21,19 C 21,20.1 20.1,21 19,21 L 5,21 C 3.9,21 3,20.1 3,19 Z",
    grid: 24.0,
    stroke: Some(2.0),
};

/// The room's link (the web's Link), stroked on its 24-unit grid: the icon
/// theme's insert-link is an arrow into a box, which reads as "insert".
pub const LINK: Shape = Shape {
    data: "M 10,13 A 5,5 0 0 0 17.54,13.54 L 20.54,10.54 A 5,5 0 0 0 13.47,3.47 L 11.75,5.18 M 14,11 A 5,5 0 0 0 6.46,10.46 L 3.46,13.46 A 5,5 0 0 0 10.53,20.53 L 12.24,18.82",
    grid: 24.0,
    stroke: Some(2.0),
};

/// The send arrow (the web's ArrowUp).
pub const ARROW_UP: Shape = Shape {
    data: "M 5,12 L 12,5 L 19,12 M 12,19 L 12,5",
    grid: 24.0,
    stroke: Some(2.4),
};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Glyph {
        pub shape: Cell<Option<Shape>>,
        pub path: RefCell<Option<gsk::Path>>,
        pub size: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Glyph {
        const NAME: &'static str = "TomoshibiRoomGlyph";
        type Type = super::Glyph;
        type ParentType = gtk::Widget;

        fn class_init(class: &mut Self::Class) {
            class.set_css_name("glyph");
            class.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for Glyph {}

    impl WidgetImpl for Glyph {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            let size = self.size.get();
            (size, size, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let (Some(shape), Some(path)) = (self.shape.get(), self.path.borrow().clone()) else {
                return;
            };
            let widget = self.obj();
            let side = widget.width().min(widget.height()) as f32;
            if side <= 0.0 {
                return;
            }
            let color = widget.color();
            let scale = side / shape.grid;
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                (widget.width() as f32 - side) / 2.0,
                (widget.height() as f32 - side) / 2.0,
            ));
            snapshot.scale(scale, scale);
            match shape.stroke {
                None => snapshot.append_fill(&path, gsk::FillRule::Winding, &color),
                Some(width) => {
                    let stroke = gsk::Stroke::new(width);
                    stroke.set_line_cap(gsk::LineCap::Round);
                    stroke.set_line_join(gsk::LineJoin::Round);
                    snapshot.append_stroke(&path, &stroke, &color);
                }
            }
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    /// A symbol drawn from a path in the widget's colour.
    pub struct Glyph(ObjectSubclass<imp::Glyph>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Glyph {
    pub fn new(shape: Shape, size: i32) -> Glyph {
        let glyph: Glyph = glib::Object::new();
        glyph.imp().size.set(size);
        glyph.set_shape(shape);
        glyph.set_halign(gtk::Align::Center);
        glyph.set_valign(gtk::Align::Center);
        glyph.set_can_target(false);
        glyph
    }

    pub fn set_shape(&self, shape: Shape) {
        if self.imp().shape.get() == Some(shape) {
            return;
        }
        // The paths are this file's constants; one that did not parse would
        // be a mistake here, drawn as nothing rather than as a crash.
        let path = gsk::Path::parse(shape.data).ok();
        self.imp().shape.set(Some(shape));
        self.imp().path.replace(path);
        self.queue_draw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_symbol_parses() {
        for shape in [HAND, CROWN, MESSAGE, ARROW_UP, CLAPPERBOARD, LINK] {
            assert!(gsk::Path::parse(shape.data).is_ok(), "{}", shape.data);
        }
    }
}
