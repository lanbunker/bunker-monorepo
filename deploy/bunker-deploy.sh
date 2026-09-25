#!/usr/bin/env bash
# sudo lets the deploy user run only this script.
set -euo pipefail

upload=/home/deploy/bunker-api.new
staging=/var/lib/bunker-deploy
bin=/usr/local/bin/bunker-api
prev=/usr/local/bin/bunker-api.prev
db=/var/lib/bunker/bunker.db
snapshot=/var/backups/bunker/pre-deploy.db

fail() {
    echo "$*" >&2
    exit 1
}

wait_ready() {
    local port=$1
    for _ in $(seq 1 30); do
        if curl -fsS "http://127.0.0.1:${port}/health/ready" >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.5
    done
    return 1
}

# One key only. Sourcing the file would put the JWT secret into this shell.
port=$(sed -n 's/^PORT=//p' /etc/bunker/api.env)
[ -n "$port" ] || fail "no PORT in /etc/bunker/api.env"
[ -f "$db" ] || fail "no database at $db. Is the /var/lib/bunker mount missing?"

# The deploy user controls its home, so the upload can be a symlink or a hard
# link to a file that only root reads. The checks run again after the move,
# because only then can the deploy user no longer swap the file.
[ ! -L "$upload" ] && [ -f "$upload" ] || fail "no regular file at $upload"
install -d -m 700 -o root -g root "$staging"
mv -T "$upload" "$staging/bunker-api"
staged=$staging/bunker-api
[ ! -L "$staged" ] && [ -f "$staged" ] || fail "the upload is not a regular file"
[ "$(stat -c '%U %h' "$staged")" = "deploy 1" ] \
    || fail "the upload must belong to deploy and have one link"

# The unit stops before the snapshot, so a rollback loses no write of the old
# binary.
systemctl stop bunker-api
runuser -u bunker -- sqlite3 -cmd '.timeout 5000' "$db" ".backup '$snapshot'"
if [ -f "$bin" ]; then
    install -m 755 -o root -g root "$bin" "$prev"
fi
install -m 755 -o root -g root "$staged" "$bin"
rm -f "$staged"

systemctl reset-failed bunker-api
systemctl start bunker-api
if wait_ready "$port"; then
    echo "bunker-api is ready"
    exit 0
fi

echo "bunker-api did not answer /health/ready within 15 seconds" >&2
journalctl -u bunker-api -n 40 --no-pager >&2
systemctl stop bunker-api
[ -f "$prev" ] || fail "DEPLOY FAILED. No previous binary, so bunker-api stays stopped."

# The new binary can apply a migration at startup that the old binary refuses,
# so the database goes back together with the binary.
install -m 755 -o root -g root "$prev" "$bin"
rm -f "$db-wal" "$db-shm"
install -m 600 -o bunker -g bunker "$snapshot" "$db"
systemctl reset-failed bunker-api
systemctl start bunker-api
if wait_ready "$port"; then
    fail "DEPLOY FAILED. The previous binary and the pre-deploy database are back."
fi
fail "DEPLOY FAILED. The rollback did not answer /health/ready either. Read journalctl -u bunker-api."
