# Art the clients share

`portraits.json` is the single source of the little animals drawn on the
phone lobby's camera card while the camera is off: a pane of glass with one
animal after another drawn on it in thin lines, stroke by stroke, the way
the iPhone writes its first "hello". Every client plays this file as it is, so that every
platform draws the same lines at the same moments. Change the drawings here
and nowhere else.

The animals are a cat, a rabbit, a bear, a hamster with a sunflower seed, a
penguin and a chick coming out of its egg: round and simple, two dots for
eyes and at most a tiny nose or mouth. The owner turned down realistic
people, then cartoon children with drawn faces and lines of glass, as eerie
(2026-10-27); keep to plain thin lines, no hollow eyes, no glints, no
lashes.

On Apple the file becomes Swift: `apple/generate-portraits.py` writes
`apple/TomoshibiKit/Sources/TomoshibiBasics/PortraitArt.swift` from it, and
`PortraitArtTests` (run by `swift test` in `apple/TomoshibiKit`) fail while
the two disagree. Run the script after every change to the JSON.

## The file

```json
{
  "format": 1,
  "canvas": {"width": 100, "height": 100},
  "lineWidth": 1.3,
  "placement": "bottom-center",
  "gap": 0.4,
  "portraits": [
    {"name": "cat", "hold": 2.0, "fade": 0.6,
     "strokes": [{"d": "M 50 67.6 C 36 67.6 26 60.4 26 49.4 ...", "start": 0.0, "duration": 1.213}, ...]}
  ]
}
```

- `format`: 1. A reader that does not know the number draws nothing.
- `canvas`: the drawing's coordinate space. Origin at the top left, y down.
- `lineWidth`: the width of every line, in canvas units. Every stroke is
  drawn at this one width, with round caps and round joins.
- `placement`: how the canvas sits on the card; see below.
- `gap`: seconds of empty glass before each animal.
- `portraits`: the animals in the order they are played. After the last
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
      cat's three stripes, or the two eyes); the pen goes through them in
      order. An eye is a very short line, about a line's width long, which
      round ends make a dot.
    - `start`: seconds after the drawing begins (after the `gap`) that the
      pen starts this stroke.
    - `duration`: seconds the stroke takes.

Strokes are drawn in order: the outline first (head and ears in one line
where they meet, or a round body), then what is inside the ears and the
markings (a tabby's stripes, a bear's muzzle, a penguin's white face, the
egg's crack and shell), then the eyes, and last the nose or mouth.

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
side) the canvas fills the width and leaves empty glass above the drawing.
Lines that leave the canvas are clipped by the card's rounded rectangle.

Keep in mind what covers the card on a phone when drawing: the Start a
meeting button and the calendar button cover the canvas below about
y = 78, and the microphone and camera switches cover its top right corner
(x > 79 and y < 18). The animals keep between y = 8 and y = 76 and out of
that corner.

## Timing

For one animal, with `t` the seconds since their turn began:

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
drawn. When the turn ends, the next animal's `gap` begins.

Play only while the card can be seen: hold still (keep the moment) while the
app is in the background, the card is scrolled away or covered by a sheet,
and resume from there. With the system's reduce-motion setting on, draw the
first animal finished and still. Nothing is redrawn while nothing moves (the
gap and the hold).

## The lines

Every stroke drawn so far is stroked once, as one shape, at
`w = lineWidth * scale` with round caps and round joins, in one colour: the
interface's text colour at an opacity. Two lines that cross are one shape, so
the colour does not double where they meet.

| | night | day |
| --- | --- | --- |
| colour | `#F5F5F7` at 60 % | `#1C1C1E` at 50 % |
| fill | none: lines only | same |
| glass, highlight, shadow | none | none |

At 1.3 canvas units the line is about 3.8 pt on an iPhone's card. Drawn by
day at 40 % the lines were faint on the glass, and by night at 72 % hard;
these are the soft ones that still read at a glance.

## The card

On Apple platforms with Liquid Glass (iOS and macOS 26 and later) the card is
Liquid Glass of the clear kind in the card's rounded rectangle; the regular
kind casts a soft grey shadow round it, which by day read as dirt. What the
clear glass comes to on the phone's lobby, measured from its pixels, is what
to draw elsewhere (Android, Windows):

| | night | day |
| --- | --- | --- |
| page round the card | `#000000` | `#F2F2F7` |
| card | `#202020` | `#FFFFFF` |
| edge | 1 pt of white at 40 % just inside the card's outline (the glass's lit rim) | none |
| shadow | none | none |

Apple before 26 draws the system's thinnest blur material with a 1 pt border
of the text colour at 16 % instead. No gradients anywhere, and nothing
filled but the lines themselves.
