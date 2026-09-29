#!/usr/bin/env bash
# `.backup` uses the SQLite online backup API, so a write in flight never lands
# in the copy. Keeps 14 days.
set -euo pipefail

src=/var/lib/bunker/bunker.db
dest="/var/backups/bunker/bunker-$(date +%F).db"

[ -f "$src" ] || { echo "no database at $src" >&2; exit 1; }
sqlite3 -cmd '.timeout 5000' "$src" ".backup '$dest'"
check=$(sqlite3 "$dest" 'pragma integrity_check')
if [ "$check" != ok ]; then
    echo "the copy at $dest fails the integrity check: $check" >&2
    exit 1
fi
gzip -f "$dest"
find /var/backups/bunker -name 'bunker-*.db.gz' -mtime +13 -delete
