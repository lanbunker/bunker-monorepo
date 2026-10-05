#!/usr/bin/env bash
# A checked copy of the database on the box, kept 14 days. `.backup` uses the
# SQLite online backup API, so a write in flight never lands in the copy.
set -euo pipefail

readonly SRC=/var/lib/bunker/bunker.db
readonly DIR=/var/backups/bunker
readonly DEST=$DIR/bunker-$(date +%F).db

[ -f "$SRC" ] || { echo "bunker-backup: no database at $SRC" >&2; exit 1; }
sqlite3 -cmd '.timeout 5000' "$SRC" ".backup '$DEST'"
check=$(sqlite3 "$DEST" 'pragma integrity_check')
if [ "$check" != ok ]; then
    echo "bunker-backup: the copy fails the integrity check: $check" >&2
    exit 1
fi
gzip -f "$DEST"
find "$DIR" -name 'bunker-*.db.gz' -mtime +13 -delete
