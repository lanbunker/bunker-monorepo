#!/usr/bin/env bash
# A consistent copy of the live database. `.backup` uses the SQLite online
# backup API, so a write in flight never lands in the copy. Keeps 14 days.
set -euo pipefail

src=/var/lib/bunker/bunker.db
dest="/var/backups/bunker/bunker-$(date +%F).db"

[ -f "$src" ] || exit 0
sqlite3 "$src" ".backup '$dest'"
gzip -f "$dest"
find /var/backups/bunker -name 'bunker-*.db.gz' -mtime +14 -delete
