# The image the AppImages are made in (appimage.sh), one per architecture:
# the build image, which already holds the very GTK, libadwaita and glib the
# app was linked against, with the tools that turn a binary into an AppImage
# on top. Its own image rather than more lines in the build image's
# Dockerfile, because that one is shared by everybody's builds and carries
# its hash as a label: a packaging tool added there would rebuild the
# toolchain for every checkout.
#
# Every tool is pinned to a release and to the SHA-256 of what that release
# handed over on the day it was chosen, so that an image made in a month is
# made by the same tools as one made today, and a release replaced under its
# own tag is refused rather than used:
#
#   linuxdeploy 1-alpha-20251107-1   copies in every library the binary and
#                                    the GTK modules need, except those every
#                                    system has (its exclude list), and points
#                                    their run paths into the image
#   appimagetool 1.9.1               makes the squashfs and puts the runtime
#                                    in front of it
#   type2-runtime 20251108           the runtime itself, given to appimagetool
#                                    as a file so that nothing is fetched
#                                    while an image is being made
#
# linuxdeploy's GTK plugin is not used. Its hook exports GDK_BACKEND=x11 and
# GTK_THEME=Adwaita:<variant> into every start: the first takes Wayland away
# from the app (a session with no XWayland cannot show it at all), and the
# second overrides libadwaita's own style manager, which is what makes the
# app follow the system between day and night. appimage-inside.sh does the
# GTK part itself.
#
# The two AppImages are extracted here, at build time, so that a container
# with no FUSE (every container) runs them as plain programs.
ARG BUILD_IMAGE
FROM ${BUILD_IMAGE}

ARG ARCH
ARG LINUXDEPLOY=1-alpha-20251107-1
ARG APPIMAGETOOL=1.9.1
ARG RUNTIME=20251108

# desktop-file-validate and appstreamcli, which appimagetool runs over the
# image's desktop entry and metadata when they are on the path (and refuses
# an image whose entry does not validate); rsvg-convert for the icon's
# fixed sizes.
RUN apt-get update \
 && apt-get install -y --no-install-recommends desktop-file-utils appstream librsvg2-bin \
 && rm -rf /var/lib/apt/lists/*

RUN set -eu; \
    case "$ARCH" in \
      x86_64) ld=c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d; \
              at=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0; \
              rt=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d ;; \
      aarch64) ld=620095110d693282b8ebeb244a95b5e911cf8f65f76c88b4b47d16ae6346fcff; \
               at=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158; \
               rt=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444 ;; \
      *) echo "unknown ARCH $ARCH" >&2; exit 1 ;; \
    esac; \
    mkdir -p /opt/appimage && cd /opt/appimage; \
    curl -fsSL -o linuxdeploy.AppImage \
      "https://github.com/linuxdeploy/linuxdeploy/releases/download/$LINUXDEPLOY/linuxdeploy-$ARCH.AppImage"; \
    curl -fsSL -o appimagetool.AppImage \
      "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL/appimagetool-$ARCH.AppImage"; \
    curl -fsSL -o runtime "https://github.com/AppImage/type2-runtime/releases/download/$RUNTIME/runtime-$ARCH"; \
    printf '%s  %s\n' "$ld" linuxdeploy.AppImage "$at" appimagetool.AppImage "$rt" runtime | sha256sum -c -; \
    chmod +x linuxdeploy.AppImage appimagetool.AppImage; \
    ./linuxdeploy.AppImage --appimage-extract >/dev/null && mv squashfs-root linuxdeploy; \
    ./appimagetool.AppImage --appimage-extract >/dev/null && mv squashfs-root appimagetool; \
    rm linuxdeploy.AppImage appimagetool.AppImage; \
    ln -s /opt/appimage/linuxdeploy/AppRun /usr/local/bin/linuxdeploy; \
    ln -s /opt/appimage/appimagetool/AppRun /usr/local/bin/appimagetool; \
    linuxdeploy --list-plugins >/dev/null; appimagetool --version
