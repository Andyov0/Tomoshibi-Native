#!/usr/bin/env bash
# The inside of appimage.sh, in the packaging image (package.Dockerfile):
# one built binary in, one AppImage out.
#
# Environment (appimage.sh sets it): ARCH, KIND (release or test), BIN (the
# built binary), WORK (the checkout, read-only), OUT (where the image goes),
# VERSION (dotted, as the update check compares it), IMAGE (its file name),
# and for a release TOMOSHIBI_HOST and TOMOSHIBI_UPDATE_KEY, which the core
# must have been built with.
#
# The GTK, libadwaita and glib that go in are this image's, which are the
# ones the binary was linked against in the build image it is made from.
set -euo pipefail

data=$WORK/linux/data
here=$WORK/linux/build
triplet=$(gcc -dumpmachine)
lib=/usr/lib/$triplet
id=app.tomoshibi.Linux

# 1. The core must be one to give out (a release), or one for nobody (a test).
if [ "$KIND" = release ]; then
  bash "$here/check-core.sh" "$BIN"
else
  bash "$here/check-core.sh" --test "$BIN"
fi

app=/tmp/Tomoshibi.AppDir
rm -rf "$app" /tmp/pack && mkdir -p "$app/usr/bin" "$app/usr/lib" "$app/usr/share" /tmp/pack
install -m 755 "$BIN" "$app/usr/bin/tomoshibi"

# 2. The desktop entry, with its two sentences in the app's other languages
# from the app's own phrases (the same ones update/desktop.rs writes into the
# entry it installs), rather than written out a second time here.
phrases=$WORK/android/app/src/main/assets/phrases.json
desktop=/tmp/pack/$id.desktop
cp "$data/$id.desktop" "$desktop"
for pair in zh-Hans:zh_CN zh-Hans:zh_SG zh-Hant:zh_TW zh-Hant:zh_HK ja:ja; do
  language=${pair%%:*} locale=${pair#*:}
  for key in GenericName Comment; do
    english=$(sed -n "s/^$key=//p" "$data/$id.desktop")
    said=$(jq -r --arg l "$language" --arg k "$english" '.[$l][$k] // empty' "$phrases")
    [ -n "$said" ] && [ "$said" != "$english" ] && printf '%s[%s]=%s\n' "$key" "$locale" "$said" >>"$desktop"
  done
done
desktop-file-validate "$desktop"

# 3. The icon: the drawing, and the fixed sizes a desktop without an SVG
# loader, or a launcher that wants a bitmap, asks for.
icons=()
for size in 16 24 32 48 64 128 256 512; do
  mkdir -p "/tmp/pack/${size}"
  rsvg-convert -w "$size" -h "$size" "$data/icons/$id.svg" -o "/tmp/pack/${size}/$id.png"
  icons+=(--icon-file "/tmp/pack/${size}/$id.png")
done
icons+=(--icon-file "$data/icons/$id.svg")

# 4. What GTK reads that is not a library (AppRun says how each is found).
# The pixbuf loaders go into usr/lib itself (linuxdeploy --library puts them
# there and points their run paths at it), and the cache names them without
# a directory, so that gmodule's dlopen finds them by its own run path
# wherever the image is mounted. The system's cache is the list of the
# loaders the build image has, which is the list of what is copied in.
pixbuf=$lib/gdk-pixbuf-2.0/2.10.0
loaders=()
for loader in "$pixbuf"/loaders/*.so; do loaders+=(--library "$loader"); done
mkdir -p "$app/usr/lib/gdk-pixbuf-2.0/2.10.0"
sed "s|$pixbuf/loaders/||g" "$pixbuf/loaders.cache" >"$app/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"
# (A loader's line is its file name alone in quotes; the magic patterns of
# the XPM and XBM loaders begin with a slash too, hence the .so.)
if grep -q '^"/.*\.so"$' "$app/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"; then
  echo "appimage: a loader in loaders.cache still has a directory" >&2; exit 1
fi

# GTK's own schemas, compiled for this GTK; the system's are read after.
mkdir -p "$app/usr/share/glib-2.0/schemas"
cp /usr/share/glib-2.0/schemas/org.gtk.gtk4.*.gschema.xml "$app/usr/share/glib-2.0/schemas/"
glib-compile-schemas "$app/usr/share/glib-2.0/schemas"

# The Adwaita icons this libadwaita draws with: its symbolic ones and the
# few others, without the cursors, which are the system's to choose and
# eleven of its fifteen megabytes.
mkdir -p "$app/usr/share/icons/Adwaita" "$app/usr/share/icons/hicolor"
for part in index.theme symbolic symbolic-up-to-32 scalable 16x16; do
  cp -a "/usr/share/icons/Adwaita/$part" "$app/usr/share/icons/Adwaita/"
done
cp /usr/share/icons/hicolor/index.theme "$app/usr/share/icons/hicolor/"

# An empty directory for GIO's modules (AppRun's GIO_MODULE_DIR).
mkdir -p "$app/usr/lib/gio/modules"
: >"$app/usr/lib/gio/modules/.none"

# 4b. GStreamer, the player of watching together (ui/room/watch): the
# plugins it plays a file or an HLS playlist with, in usr/lib/gstreamer-1.0,
# and the scanner GStreamer runs to read them. linuxdeploy below copies what
# they link and points their run paths into the image (--deploy-deps-only).
# libav is not among them: Ubuntu's FFmpeg links every codec it was built
# with, about 126 MB more on aarch64; H.264 is decoded by OpenH264 and AAC by
# FAAD instead, VP8 and VP9 by libvpx, Opus and Vorbis by theirs. libsoup is
# named, since the HTTP source opens it at run time rather than linking it.
# The app points GStreamer at these, and at a registry of its own, when it
# runs from the image (ui/room/watch/player.rs `started`).
gst=$lib/gstreamer-1.0
mkdir -p "$app/usr/lib/gstreamer-1.0" "$app/usr/lib/gstreamer1.0/gstreamer-1.0"
for plugin in coreelements playback typefindfunctions app videoconvertscale audioconvert \
  audioresample volume autodetect pulseaudio isomp4 matroska ogg vorbis opus vpx audioparsers \
  id3demux soup hls adaptivedemux2 mpegtsdemux videoparsersbad openh264 faad; do
  cp -L "$gst/libgst$plugin.so" "$app/usr/lib/gstreamer-1.0/"
done
cp -L "$lib/gstreamer1.0/gstreamer-1.0/gst-plugin-scanner" "$app/usr/lib/gstreamer1.0/gstreamer-1.0/"
gstreamer=(--deploy-deps-only "$app/usr/lib/gstreamer-1.0"
  --deploy-deps-only "$app/usr/lib/gstreamer1.0/gstreamer-1.0"
  --library "$lib/libsoup-3.0.so.0")

# 5. The libraries. linuxdeploy copies every one the binary and the loaders
# need into usr/lib, except the ones its list says every system has and must
# be the system's own (glibc, the GL drivers, fontconfig and freetype, X11
# and xcb, libgcc_s and so on). Wayland's client libraries are left to the
# system as well: Mesa's EGL, which is always the system's, is built against
# the system's libwayland-client and asks it for newer symbols than 24.04's,
# and an older one loaded first from here would make GTK's GL fail on a
# newer system. Every Wayland desktop has them, since Mesa needs them.
linuxdeploy --appdir "$app" \
  --executable "$app/usr/bin/tomoshibi" \
  --desktop-file "$desktop" \
  "${icons[@]}" \
  "${loaders[@]}" \
  "${gstreamer[@]}" \
  --custom-apprun "$here/AppRun" \
  --exclude-library 'libwayland-client.so*' \
  --exclude-library 'libwayland-cursor.so*' \
  --exclude-library 'libwayland-egl.so*' \
  --exclude-library 'libwayland-server.so*'

# One library on that list is copied in all the same: libgpg-error, which
# the libgcrypt copied in needs and which a plain Debian 13 no longer has
# (its apt checks signatures with sqv rather than gpgv, and nothing else in
# a base system pulls it in): the image did not start there without it
# ("libgpg-error.so.0: cannot open shared object file"). linuxdeploy skips
# a library on its list even when it is named, so it is copied here; it
# needs nothing but the C library.
cp -L "$lib/libgpg-error.so.0" "$app/usr/lib/libgpg-error.so.0"
mkdir -p "$app/usr/share/doc/libgpg-error0"
cp /usr/share/doc/libgpg-error0/copyright "$app/usr/share/doc/libgpg-error0/"

gtk4-update-icon-cache -q -t -f "$app/usr/share/icons/Adwaita"
gtk4-update-icon-cache -q -t -f "$app/usr/share/icons/hicolor"

mkdir -p "$app/usr/share/metainfo"
sed -e "s/@VERSION@/$VERSION/" -e "s/@DATE@/$(date -u +%Y-%m-%d)/" \
  "$data/$id.metainfo.xml" >"$app/usr/share/metainfo/$id.metainfo.xml"
appstreamcli validate --no-net --explain "$app/usr/share/metainfo/$id.metainfo.xml"

# Nothing of the build machine's Wayland libraries got in by another way.
if ls "$app/usr/lib" | grep -q '^libwayland-'; then
  echo "appimage: a Wayland library was copied in" >&2; exit 1
fi

# 6. The image: the runtime given as a file, so that nothing is fetched now.
rm -f "$OUT/$IMAGE"
ARCH=$ARCH appimagetool --runtime-file /opt/appimage/runtime "$app" "$OUT/$IMAGE.part" >/tmp/pack/appimagetool.log 2>&1 \
  || { cat /tmp/pack/appimagetool.log >&2; exit 1; }
chmod 755 "$OUT/$IMAGE.part"
mv "$OUT/$IMAGE.part" "$OUT/$IMAGE"

# What went in, beside it, for whoever has to answer "which libraries does
# the image carry".
( cd "$app" && find . -type f -o -type l | sort ) >"$OUT/$IMAGE.contents.txt"
size=$(stat -c %s "$OUT/$IMAGE")
sha=$(sha256sum "$OUT/$IMAGE" | cut -d' ' -f1)
echo "made $IMAGE: $size bytes, sha256 $sha"
