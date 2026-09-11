#!/usr/bin/env bash
# Installs the binary that GitHub Actions uploaded and restarts the service.
# sudo lets the deploy user run only this script with no arguments, so a leaked
# CI key can ship a binary and do nothing else.
set -euo pipefail

new=/home/deploy/bunker-api.new
[ -f "$new" ] || { echo "no upload at $new" >&2; exit 1; }

install -m 755 -o root -g root "$new" /usr/local/bin/bunker-api
rm -f "$new"
systemctl restart bunker-api

# One key only. Sourcing the file would put the JWT secret into this shell.
port=$(sed -n 's/^PORT=//p' /etc/bunker/api.env)
[ -n "$port" ] || { echo "no PORT in /etc/bunker/api.env" >&2; exit 1; }
for _ in $(seq 1 30); do
    if curl -fsS "http://127.0.0.1:${port}/health/ready" >/dev/null 2>&1; then
        echo "bunker-api is ready"
        exit 0
    fi
    sleep 0.5
done

echo "bunker-api did not answer /health/ready within 15 seconds" >&2
journalctl -u bunker-api -n 40 --no-pager >&2
exit 1
