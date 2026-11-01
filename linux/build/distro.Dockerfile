# A plain distribution with a compositor and nothing of the app's: what
# distro-check.sh starts an AppImage in, to see it run on a system that
# never had GTK 4 or libadwaita installed. One per distribution and
# architecture, from the distribution's own image by digest (common.sh says
# why a digest).
#
# What goes in is what any desktop has under an app and an AppImage leaves
# to it: weston (which brings Mesa, fontconfig, cairo and the Wayland
# libraries with it), a font, and a session bus for the app's single
# instance. Recommended packages are left out, so that nothing more of a
# desktop arrives by the back door; the run then checks that no GTK 4 and
# no libadwaita are on the system at all.
ARG BASE
FROM ${BASE}

RUN set -eu; \
    if command -v apt-get >/dev/null; then \
      export DEBIAN_FRONTEND=noninteractive; \
      apt-get update; \
      apt-get install -y --no-install-recommends weston dbus fonts-dejavu-core; \
      rm -rf /var/lib/apt/lists/*; \
    else \
      dnf -y --setopt=install_weak_deps=False install weston dbus-daemon dejavu-sans-fonts which findutils; \
      dnf clean all; \
    fi; \
    command -v weston-screenshooter; command -v dbus-run-session
