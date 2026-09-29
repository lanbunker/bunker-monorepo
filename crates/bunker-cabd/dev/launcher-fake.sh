#!/bin/bash
# Stands in for runcommand.sh on a desktop. Waits a moment, as a game would,
# then writes a random score in the format the `ascii_decimal` decoder reads.
# Six digits with leading zeros, the same size as the template, because a
# file of another size than the template is read as a cut write.
#   $1  the ROM path
#   $2  the live hiscore file
set -eu
rom="$1"
hi="$2"
echo "fake launcher: $rom" >&2
sleep 2
printf '%06d' "$((RANDOM * 7 + 1000))" > "$hi"
echo "fake launcher: wrote $(cat "$hi") to $hi" >&2
