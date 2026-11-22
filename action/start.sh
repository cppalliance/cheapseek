#!/usr/bin/env bash
# Starts a CheapSeek server that runs for the rest of a GitHub Actions job,
# and hands its base URL to the steps that follow.
#
# `action.yml` runs this script. Two settings come in through the
# environment:
#   CHEAPSEEK_PORT        the port to listen on; 0 takes a free port
#   CHEAPSEEK_EXECUTABLE  an executable to start; when it is empty, the
#                         script downloads the one of this action's release
set -euo pipefail

# The repository whose releases hold the executables. After a rename of the
# repository, the name here has to follow.
repository=cppalliance/cheapseek

fail() {
  echo "cheapseek action: $*" >&2
  exit 1
}

# Prints the SHA-256 of a file in hex. The file comes in on stdin: given a
# path with a backslash in it, sha256sum escapes its whole output line.
sha256() {
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum < "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 < "$1" | cut -d ' ' -f 1
  fi
}

# The folder that holds `action.yml`, and beside it the `Cargo.toml` of the
# same commit.
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

# Each start gets a folder of its own, so one job can start two servers.
# `cd` and `pwd` turn the Windows path of the runner into the form bash uses.
temp=$(cd "${RUNNER_TEMP:-${TMPDIR:-/tmp}}" && pwd)
work=$(mktemp -d "$temp/cheapseek.XXXXXX")

port=${CHEAPSEEK_PORT:-0}
[[ "$port" =~ ^[0-9]+$ ]] || fail "the port has to be a number, and it is '$port'"

executable=${CHEAPSEEK_EXECUTABLE:-}
if [ -z "$executable" ]; then
  case "${RUNNER_OS:-}-${RUNNER_ARCH:-}" in
    Linux-X64) asset=cheapseek-x86_64-unknown-linux-gnu ;;
    macOS-ARM64) asset=cheapseek-aarch64-apple-darwin ;;
    Windows-X64) asset=cheapseek-x86_64-pc-windows-msvc.exe ;;
    *)
      fail "a release holds executables for Linux on x64, macOS on arm64 and" \
        "Windows on x64, and this runner is ${RUNNER_OS:-an unknown system} on" \
        "${RUNNER_ARCH:-an unknown processor}. Build CheapSeek in the job and" \
        "pass its path as the input 'executable'."
      ;;
  esac

  # The action and the executable come from one commit: the version in the
  # `Cargo.toml` beside `action.yml` names the release to download.
  version=$(awk -F '"' '/^version = "/ { print $2; exit }' "$root/Cargo.toml")
  [ -n "$version" ] || fail "found no version in $root/Cargo.toml"
  release=https://github.com/$repository/releases/download/v$version

  for name in SHA256SUMS "$asset"; do
    curl --fail --location --silent --show-error --retry 3 \
      --output "$work/$name" "$release/$name" ||
      fail "could not download $release/$name"
  done

  # A line of SHA256SUMS is the sum, then the file name, which has a `*` in
  # front when the sum was taken in binary mode.
  want=$(tr -d '\r' < "$work/SHA256SUMS" |
    awk -v name="$asset" '{ file = $2; sub(/^\*/, "", file); if (file == name) print $1 }')
  [ -n "$want" ] || fail "SHA256SUMS of release v$version does not list $asset"
  got=$(sha256 "$work/$asset")
  [ "$got" = "$want" ] ||
    fail "$asset of release v$version has the SHA-256 $got, and SHA256SUMS says $want"

  executable=$work/$asset
  chmod +x "$executable"
fi
[ -f "$executable" ] || fail "found no executable at $executable"

# All three standard streams of the server go to files. A server that held
# the output pipe of this step would keep the step open, and the runner
# stops such a process a few seconds after the step ends.
: > "$work/stdout.txt"
nohup "$executable" --port "$port" \
  > "$work/stdout.txt" 2> "$work/stderr.txt" < /dev/null &
server=$!

# The server prints one line, its base URL, when it is ready. `read` fails
# until the whole line is in the file.
url=
for _ in $(seq 1 1200); do
  if IFS= read -r url < "$work/stdout.txt"; then
    break
  fi
  url=
  kill -0 "$server" 2> /dev/null || break
  sleep 0.1
done
url=${url%$'\r'}

if ! [[ "$url" =~ ^http://127\.0\.0\.1:[0-9]+/v1$ ]]; then
  kill "$server" 2> /dev/null || true
  cat "$work/stderr.txt" >&2
  fail "the server did not print its base URL"
fi

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  echo "base-url=$url" >> "$GITHUB_OUTPUT"
fi
if [ -n "${GITHUB_ENV:-}" ]; then
  echo "CHEAPSEEK_BASE_URL=$url" >> "$GITHUB_ENV"
fi
echo "CheapSeek serves at $url"
