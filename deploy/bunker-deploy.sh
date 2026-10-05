#!/usr/bin/env bash
# Installs the binary that CI uploaded and restarts the API. When the new
# binary is not ready, the old binary and the database from before the deploy
# come back. sudo lets the deploy user run only this script.
set -euo pipefail

readonly UPLOAD=/home/deploy/bunker-api.new
readonly STAGING=/var/lib/bunker-deploy
readonly BIN=/usr/local/bin/bunker-api
readonly PREV=/usr/local/bin/bunker-api.prev
readonly DATA=/var/lib/bunker
readonly DB=$DATA/bunker.db
readonly SNAPSHOT=/var/backups/bunker/pre-deploy.db

fail() {
    echo "bunker-deploy: $*" >&2
    exit 1
}

wait_ready() {
    for _ in $(seq 1 30); do
        curl -fsS "http://127.0.0.1:$PORT/health/ready" >/dev/null 2>&1 && return 0
        sleep 0.5
    done
    return 1
}

start_api() {
    systemctl reset-failed bunker-api
    systemctl start bunker-api
    wait_ready
}

# The deploy user owns its home, so the upload can be a symlink or a hard link
# to a file that only root can read. The checks run again after the move,
# because only then can the deploy user no longer swap the file.
stage_upload() {
    [ ! -L "$UPLOAD" ] && [ -f "$UPLOAD" ] || fail "no regular file at $UPLOAD"
    install -d -m 700 -o root -g root "$STAGING"
    mv -T "$UPLOAD" "$STAGING/bunker-api"
    [ ! -L "$STAGING/bunker-api" ] && [ -f "$STAGING/bunker-api" ] \
        || fail "the upload is not a regular file"
    [ "$(stat -c '%U %h' "$STAGING/bunker-api")" = "deploy 1" ] \
        || fail "the upload must belong to deploy and have one link"
}

# The API stops before the snapshot, so a rollback loses no write of the old
# binary.
swap_binary() {
    systemctl stop bunker-api
    runuser -u bunker -- sqlite3 -cmd '.timeout 5000' "$DB" ".backup '$SNAPSHOT'"
    [ ! -f "$BIN" ] || install -m 755 -o root -g root "$BIN" "$PREV"
    install -m 755 -o root -g root "$STAGING/bunker-api" "$BIN"
    rm -f "$STAGING/bunker-api"
}

# The new binary can apply a migration that the old one refuses, so the
# database goes back with the binary. Litestream does not notice a replaced
# file, so its local state goes too, and it starts a new generation in B2.
roll_back() {
    systemctl stop bunker-api litestream
    install -m 755 -o root -g root "$PREV" "$BIN"
    rm -rf "$DB-wal" "$DB-shm" "$DATA/.bunker.db-litestream"
    install -m 600 -o bunker -g bunker "$SNAPSHOT" "$DB"
    systemctl start litestream
}

# One deploy at a time. A second one waits for the first.
exec 9>/run/bunker-deploy.lock
flock 9

# Only one key. Sourcing the file would put the JWT secret into this shell.
PORT=$(sed -n 's/^PORT=//p' /etc/bunker/api.env)
[ -n "$PORT" ] || fail "no PORT in /etc/bunker/api.env"
[ -f "$DB" ] || fail "no database at $DB. Is the volume mounted?"

stage_upload
swap_binary
if start_api; then
    echo "bunker-deploy: the new binary is ready"
    exit 0
fi

echo "bunker-deploy: the new binary did not answer /health/ready in 15 seconds" >&2
journalctl -u bunker-api -n 40 --no-pager >&2
[ -f "$PREV" ] || { systemctl stop bunker-api; fail "no previous binary. The API stays stopped."; }

roll_back
start_api || fail "the rollback did not answer /health/ready either. Read journalctl -u bunker-api."
fail "the old binary and the database from before the deploy are back"
