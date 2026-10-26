# Art the clients share

`portraits.json` is the single source of the cartoon children drawn on the
phone lobby's camera card while the camera is off: a pane of glass with one
child after another drawn on it in lines of glass, stroke by stroke, the way
the iPhone writes its first "hello". Every client plays this file as it is,
so that every platform draws the same lines at the same moments. Change the
drawings here and nowhere else.

On Apple the file becomes Swift: `apple/generate-portraits.py` writes
`apple/TomoshibiKit/Sources/TomoshibiBasics/PortraitArt.swift` from it, and
`PortraitArtTests` (run by `swift test` in `apple/TomoshibiKit`) fail while
the two disagree. Run the script after every change to the JSON.

## The file

```json
{
  "format": 1,
  "canvas": {"width": 100, "height": 100},
  "lineWidth": 2.1,
  "placement": "bottom-center",
  "gap": 0.35,
  "portraits": [
    {"name": "cowlick", "hold": 1.7, "fade": 0.6,
     "strokes": [{"d": "M 26.4 46 C 26.4 62 37 70 50 70 ...", "start": 0, "duration": 0.317}, ...]}
  ]
}
```

- `format`: 1. A reader that does not know the number draws nothing.
- `canvas`: the drawing's coordinate space. Origin at the top left, y down.
- `lineWidth`: the width of every line, in canvas units. Every stroke is
  drawn at this one width, with round caps and round joins.
- `placement`: how the canvas sits on the card; see below.
- `gap`: seconds of empty glass before each child.
- `portraits`: the children in the order they are played. After the last
  comes the first again.
  - `name`: a stable identifier (lowercase ASCII). Debug switches use it
    (`TOMOSHIBI_DEBUG_PORTRAIT=<name>` on Apple).
  - `hold`: seconds the finished drawing stays before it fades.
  - `fade`: seconds the fade takes.
  - `strokes`: in the order the pen draws them.
    - `d`: SVG path data, absolute commands only: `M x y`, `L x y`,
      `C x1 y1 x2 y2 x y`, `Q x1 y1 x y`, `Z`. Every segment carries its own
      command letter; numbers are separated by single spaces, use `.` as the
      decimal point, have at most two decimals and never an exponent. A
      stroke may hold more than one `M` (marks drawn in one go, such as the
      three lines of a blush); the pen goes through them in order.
    - `start`: seconds after the drawing begins (after the `gap`) that the
      pen starts this stroke.
    - `duration`: seconds the stroke takes.

Strokes are drawn in order: the face first (cheeks and chin, or the opening
of a hood), then the hair, then the eyes (left, then right: the eye, its
upper lid, a girl's lashes, the two glints), the mouth, the blush, and last
what is worn (hood, ears on a hood, collar, ties, strings).

## Placement: `bottom-center`

The canvas is scaled uniformly to the card and stands on the card's foot:

```
inset  = 12 pt on the left and on the right (none at the top or foot)
scale  = min(cardHeight / canvas.height, (cardWidth - 2 * inset) / canvas.width)
x      = cardLeft + (cardWidth - canvas.width * scale) / 2
y      = cardBottom - canvas.height * scale
```

On a phone the card is wider than tall, so the canvas fills the card's
height. On a card taller than wide (an iPhone Duo's inner display on its
side) the canvas fills the width and leaves empty glass above the head.
Lines that leave the canvas (the shoulders run off its foot) are clipped by
the card's rounded rectangle.

Keep in mind what covers the card on a phone when drawing: the Start a
meeting button and the calendar button cover the canvas below about
y = 78, and the microphone and camera switches cover its top right corner
(x > 79 and y < 18). Faces stay above y = 72; nothing sits in that corner.

## Timing

For one child, with `t` the seconds since their turn began:

```
smoothstep(u) = u * u * (3 - 2 * u)              for u clamped to [0, 1]
tau           = t - gap                          (nothing is drawn while tau < 0)
writing       = max over strokes of (start + duration)
drawn(stroke) = smoothstep((tau - start) / duration)
opacity       = 1 - smoothstep((tau - writing - hold) / fade)
turn          = gap + writing + hold + fade
```

`drawn` is the share of the stroke's length that is down, measured along
the path from its start (trim the path by arc length; for a stroke with
several `M`s, the length is the sum of its pieces and the pen goes through
them in order). It eases in and out, so the pen slows into the end of each
line and out of its start. The drawing does not fade in: it arrives by being
drawn. When the turn ends, the next child's `gap` begins.

Play only while the card can be seen: hold still (keep the moment) while the
app is in the background, the card is scrolled away or covered by a sheet,
and resume from there. With the system's reduce-motion setting on, draw the
first child finished and still. Nothing is redrawn while nothing moves (the
gap and the hold).

## Lines of glass

The lines are drawn as one shape: every stroke drawn so far stroked at
`w = lineWidth * scale` with round caps and joins, all of them united (two
lines that cross make one shape, not a double edge). The card under them is
a pane of glass. Text colour below is the interface's own: night `#F5F5F7`,
day `#1C1C1E`.

On Apple platforms with Liquid Glass (iOS and macOS 26 and later):

| | night | day |
| --- | --- | --- |
| line | Liquid Glass (regular) in the lines' shape, tinted with the text colour at 16 % | same |
| highlight | the strokes' centre lines again, white at 62 %, width `max(0.16 w, 0.8 pt)`, moved by `(-0.17 w, -0.2 w)` (up and to the left), clipped to the lines' shape | same |
| card | Liquid Glass (regular) in the card's rounded rectangle | same |

Everywhere else (Android, Windows, and Apple before 26), which draws the
glass the way it looks without its depth:

| | night | day |
| --- | --- | --- |
| line fill | text colour at 12 % | text colour at 12 % |
| line edge | 0.75 pt stroke of the lines' shape, text colour at 34 % | same |
| highlight | as above: white at 62 %, `max(0.16 w, 0.8 pt)` wide, moved `(-0.17 w, -0.2 w)`, clipped to the lines' shape | same |
| shadow | none | none |
| card | the system's thinnest blur material, a 1 pt border of the text colour at 16 % | same |

No gradients anywhere, and nothing filled but the lines themselves: eyes,
glints and blushes are lines too.
