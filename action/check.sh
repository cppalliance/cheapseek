#!/usr/bin/env bash
# Checks a running CheapSeek server against recorded replies.
#
#   action/check.sh BASE_URL
#
# Each `NAME.request.json` in `action/check/` is sent to the server as it is,
# and the reply has to match `NAME.reply.json` byte for byte. The same
# request gets the same bytes from every build on every system, so one
# recorded file fits them all.
set -euo pipefail

base_url=${1:-${CHEAPSEEK_BASE_URL:-}}
if [ -z "$base_url" ]; then
  echo "usage: check.sh BASE_URL" >&2
  exit 2
fi

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

failed=0
for request in "$here"/check/*.request.json; do
  name=$(basename "$request" .request.json)
  want=$here/check/$name.reply.json
  got=$work/$name.reply.json

  # The request goes in on stdin, so curl never has to read a path that bash
  # on Windows writes in its own form.
  status=$(curl --silent --show-error --output "$got" --write-out '%{http_code}' \
    --header 'content-type: application/json' --data-binary @- \
    "$base_url/chat/completions" < "$request") || status="no answer"

  if [ "$status" = 200 ] && cmp -s "$want" "$got"; then
    echo "same       $name"
    continue
  fi
  failed=1
  echo "different  $name (status $status)"
  echo "  recorded: $(cat "$want")"
  if [ -f "$got" ]; then
    echo "  received: $(cat "$got")"
  fi
done

if [ "$failed" -ne 0 ]; then
  echo "the server at $base_url does not give the recorded replies" >&2
  exit 1
fi
