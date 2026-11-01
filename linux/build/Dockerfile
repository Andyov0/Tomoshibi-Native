# The image the Linux app is built, tested and screenshotted in, one per
# architecture (build.sh passes --platform). Ubuntu 24.04 because it is the
# oldest system the app supports: GTK 4.14, libadwaita 1.5 and glibc 2.39. A
# binary linked here runs on that and anything newer; one linked on a newer
# base would ask for symbols 24.04 does not have.
#
# Nothing of the source is copied in. build.sh mounts the checkout, and keeps
# cargo's registry and the target directory in named volumes, so that the
# image is the toolchain alone and is rebuilt only when this file changes.

# The base for one architecture, by digest. build.sh passes the digest of
# ubuntu:24.04's image for the architecture it builds: the build VM has no
# buildx, and the legacy builder takes the local ubuntu:24.04 (the VM's own
# arm64) for an amd64 build whatever --platform says, which installed arm64
# packages into the x86_64 image.
ARG BASE=ubuntu:24.04
FROM ${BASE}

ARG RUST_VERSION=1.99.0
# webrtc-sys compiles its bridge against the hermetic libc++ that the prebuilt
# WebRTC was built with, which is clang-only and states its own minimum clang
# (webrtc-sys build.rs, check_clang_version). GCC would build and then corrupt
# every std::unique_ptr crossing the bridge, because it ignores the
# trivial_abi annotations that change how one is returned. Ubuntu 24.04's own
# clang is 18, below that floor, so clang comes from apt.llvm.org.
ARG LLVM_VERSION=21

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8

# The libraries the app links (GTK, libadwaita, PulseAudio, PipeWire,
# libsecret, D-Bus), the headers webrtc-sys reads (glib, gobject and gio; the
# X11, DRM and GBM libraries its desktop capturer loads lazily are headers
# here and nothing at run time), V4L2 for the camera's fallback, and what a
# headless run needs: weston as the compositor, a session bus for the single
# instance and the portals, fonts for all four languages, the icon theme, and
# gdk-pixbuf's SVG loader (librsvg2-common), without which every icon the
# theme has only as SVG, which is all of its symbolic ones, is drawn as a
# missing image.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates curl gnupg git file xz-utils unzip jq \
      build-essential pkg-config \
      libgtk-4-dev libadwaita-1-dev libglib2.0-dev \
      libpulse-dev libpipewire-0.3-dev libspa-0.2-dev \
      libsecret-1-dev libdbus-1-dev \
      libx11-dev libxext-dev libxfixes-dev libxdamage-dev libxrandr-dev libxcomposite-dev \
      libdrm-dev libgbm-dev libv4l-dev \
      weston dbus dbus-user-session \
      fonts-cantarell fonts-noto-core fonts-noto-cjk adwaita-icon-theme librsvg2-common \
      libgtk-4-bin \
 && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://apt.llvm.org/llvm-snapshot.gpg.key \
      | gpg --dearmor -o /usr/share/keyrings/apt.llvm.org.gpg \
 && echo "deb [signed-by=/usr/share/keyrings/apt.llvm.org.gpg] http://apt.llvm.org/noble/ llvm-toolchain-noble-${LLVM_VERSION} main" \
      > /etc/apt/sources.list.d/llvm.list \
 && apt-get update \
 && apt-get install -y --no-install-recommends \
      clang-${LLVM_VERSION} libclang-${LLVM_VERSION}-dev lld-${LLVM_VERSION} \
 && rm -rf /var/lib/apt/lists/*

# Only C++ goes through the newer clang: webrtc-sys reads CXX, and the C the
# other crates compile is left to the system's gcc as on any Ubuntu machine.
# bindgen (PipeWire's and SPA's bindings) finds libclang here.
ENV CXX=clang++-${LLVM_VERSION} \
    LIBCLANG_PATH=/usr/lib/llvm-${LLVM_VERSION}/lib

# Everything is linked with lld rather than GNU ld. Every binary that links
# the core carries the prebuilt WebRTC, and GNU ld needed so much memory for
# each that the core's test binaries, linked side by side, were killed by
# the VM running out of it (ld terminated with signal 9); lld links the
# same in a fraction of the memory and the time. gcc stays the driver and
# finds lld by its plain name.
RUN ln -s /usr/lib/llvm-${LLVM_VERSION}/bin/ld.lld /usr/local/bin/ld.lld
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=lld" \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=lld"

# Rust at a fixed version, so that both architectures and every branch's build
# agree. RUSTUP_HOME and the cargo binaries are in the image; the registry and
# git checkouts under CARGO_HOME are volumes that build.sh mounts over them.
ENV RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs -o /tmp/rustup-init.sh \
 && sh /tmp/rustup-init.sh -y --no-modify-path --profile minimal \
      --default-toolchain ${RUST_VERSION} --component clippy,rustfmt \
 && rm /tmp/rustup-init.sh \
 && chmod -R a+rwX /opt/cargo /opt/rustup \
 && rustc --version && cargo --version && ${CXX} --version | head -1

WORKDIR /work
