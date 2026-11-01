# Shared by build.sh, run-headless.sh and server.sh; sourced, not run.

# The ubuntu:24.04 image for each architecture, by digest (Dockerfile says
# why the legacy builder needs it). Moving to a newer 24.04 point release is
# changing these two lines.
UBUNTU_ARM64=ubuntu:24.04@sha256:19cad59657037cffa0ac9a48107019b768900ff1071140965827513ac866bd5e
UBUNTU_AMD64=ubuntu:24.04@sha256:78d4744ead1f5b19b04d0df558ab182a5300903900755e515b2ca4d10eb2044d

platform_for() {
  case $1 in
    aarch64) platform=linux/arm64; base=$UBUNTU_ARM64 ;;
    x86_64) platform=linux/amd64; base=$UBUNTU_AMD64 ;;
    *) echo "unknown architecture: $1 (aarch64 or x86_64)" >&2; exit 64 ;;
  esac
}

# Builds tomoshibi-linux-build:<arch> when it is missing or was built from
# another Dockerfile: the image carries the Dockerfile's hash as a label.
ensure_image() {
  local arch=$1 wanted have
  wanted=$(shasum -a 256 "$here/Dockerfile" 2>/dev/null || sha256sum "$here/Dockerfile")
  wanted=${wanted%% *}
  have=$("${docker[@]}" image inspect "tomoshibi-linux-build:$arch" --format '{{ index .Config.Labels "tomoshibi.dockerfile" }}' 2>/dev/null || true)
  if [ "$have" != "$wanted" ]; then
    echo "building the image tomoshibi-linux-build:$arch" >&2
    "${docker[@]}" build --platform "$platform" --build-arg "BASE=$base" \
      --label "tomoshibi.dockerfile=$wanted" \
      -t "tomoshibi-linux-build:$arch" -f "$here/Dockerfile" "$here" >&2
  fi
}

# A checkout's directory name made safe for a container path and a container
# name: ASCII letters, digits, dot, dash and underscore.
checkout_name() {
  local name
  # basename's own newline is left out with printf, or tr makes it a
  # trailing underscore.
  name=$(printf '%s' "$(basename "$1")" | LC_ALL=C tr -c 'A-Za-z0-9._-' '_')
  printf '%s' "${name:-checkout}"
}
