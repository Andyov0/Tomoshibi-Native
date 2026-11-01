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

# Two libraries are lent rather than given: each in a folder of its own,
# which AppRun puts on the library path only where the system has no copy
# of its own (libpipewire), or only an older one (libstdc++). AppRun says
# why each must be the system's where the system has it. libpipewire needs
# nothing but the C library.
lend=$app/usr/lib/lend
mkdir -p "$lend/pipewire" "$lend/libstdc++"
cp -L "$lib/libpipewire-0.3.so.0" "$lend/pipewire/libpipewire-0.3.so.0"
cp -L "$lib/libstdc++.so.6" "$lend/libstdc++/libstdc++.so.6"
for package in libpipewire-0.3-0t64 libstdc++6; do
  mkdir -p "$app/usr/share/doc/$package"
  cp "/usr/share/doc/$package/copyright" "$app/usr/share/doc/$package/"
done

# What the system must have, worked out from what the image's programs and
# libraries ask of it (the versions they need of the C library and of
# libstdc++, not the ones they define), for AppRun to check before it
# starts anything.
asked() {
  find "$app/usr/bin" "$app/usr/lib" -type f ! -path "$lend/libstdc++/*" \
    \( -name '*.so*' -o -perm -u+x \) -print0 \
    | xargs -0 objdump -T 2>/dev/null | awk '/\*UND\*/' \
    | grep -o "$1[0-9.]*" | sed "s/^$1//" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1
}
mkdir -p "$app/usr/share/tomoshibi"
needs_glibc=$(asked GLIBC_)
needs_glibcxx=$(asked GLIBCXX_)
[ -n "$needs_glibc" ] && [ -n "$needs_glibcxx" ] || { echo "appimage: could not read what the image needs" >&2; exit 1; }
printf 'NEEDS_GLIBC=%s\nNEEDS_GLIBCXX=%s\n' "$needs_glibc" "$needs_glibcxx" >"$app/usr/share/tomoshibi/needs"
echo "the image needs glibc $needs_glibc and GLIBCXX_$needs_glibcxx"
# What AppRun says on an older system, in the app's four languages, from
# the app's own phrases (apple/i18n/native.json).
too_old="This computer's system is too old for Tomoshibi. It needs the GNU C library {needed} or newer (Ubuntu 24.04, Debian 13, Fedora 40 or later); this one has {found}."
for language in en zh-Hans zh-Hant ja; do
  said=$(jq -r --arg k "$too_old" --arg l "$language" '.phrases[$k][$l] // empty' "$WORK/apple/i18n/native.json")
  printf '%s\n' "${said:-$too_old}" >"$app/usr/share/tomoshibi/too-old.$language"
done

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
