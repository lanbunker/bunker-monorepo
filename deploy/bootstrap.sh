#!/usr/bin/env bash
# Prepares a fresh Debian 12 container for the API. Run it as root inside the
# container, one time. A second run changes nothing that is already in place.
#
#   bash bootstrap.sh
#
# It asks for what it cannot find. TUNNEL_TOKEN is the long string from the
# tunnel page in the Cloudflare dashboard, needed only until cloudflared is
# installed. DEPLOY_PUBKEY is the public half of the key GitHub Actions uses,
# read from /root/deploy_key.pub when that file exists. Both can also come from
# the environment.
set -euo pipefail

CLOUDFLARED_UNIT=/etc/systemd/system/cloudflared.service

# `read -p` shows nothing without a terminal, so `ssh host cmd` would wait in
# silence. Ask for `ssh -t` instead.
ask() {
    local prompt=$1 var=$2 hidden=${3:-}
    if [ ! -t 0 ]; then
        echo "no terminal to ask for the $prompt." >&2
        echo "run it as 'ssh -t root@<ip> bash /root/deploy/bootstrap.sh', or set $var" >&2
        exit 1
    fi
    if [ -n "$hidden" ]; then
        read -r -s -p "$prompt: " "$var"
        echo
    else
        read -r -p "$prompt: " "$var"
    fi
}

if [ -z "${TUNNEL_TOKEN:-}" ] && [ ! -f "$CLOUDFLARED_UNIT" ]; then
    ask "tunnel token (from the Cloudflare tunnel page)" TUNNEL_TOKEN hidden
    [ -n "$TUNNEL_TOKEN" ] || { echo "the tunnel token is empty" >&2; exit 1; }
fi
if [ -z "${DEPLOY_PUBKEY:-}" ] && [ -f /root/deploy_key.pub ]; then
    DEPLOY_PUBKEY=$(cat /root/deploy_key.pub)
fi
if [ -z "${DEPLOY_PUBKEY:-}" ]; then
    ask "deploy public key (content of deploy_key.pub)" DEPLOY_PUBKEY
fi
case "$DEPLOY_PUBKEY" in
    ssh-ed25519\ *|ssh-rsa\ *) ;;
    *) echo "the public key must start with ssh-ed25519 or ssh-rsa" >&2; exit 1 ;;
esac

here=$(cd "$(dirname "$0")" && pwd)
export DEBIAN_FRONTEND=noninteractive

apt-get update -q
apt-get install -y -q --no-install-recommends \
    ca-certificates curl openssh-server openssl sqlite3 sudo

if ! command -v cloudflared >/dev/null; then
    install -d -m 755 /usr/share/keyrings
    curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg \
        -o /usr/share/keyrings/cloudflare-main.gpg
    echo "deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared bookworm main" \
        > /etc/apt/sources.list.d/cloudflared.list
    apt-get update -q
    apt-get install -y -q cloudflared
fi
# The token holds the tunnel credentials. The unit it writes connects on boot.
# The token lands in that unit file, readable by every local account. The two
# system accounts here are trusted with it.
if [ ! -f "$CLOUDFLARED_UNIT" ]; then
    cloudflared service install "$TUNNEL_TOKEN"
fi

# `bunker` runs the API and owns the database. `deploy` is the only account
# GitHub Actions can log into, and sudo lets it run one script.
id -u bunker >/dev/null 2>&1 \
    || useradd --system --home-dir /var/lib/bunker --shell /usr/sbin/nologin bunker
id -u deploy >/dev/null 2>&1 \
    || useradd --create-home --shell /bin/bash deploy

install -d -m 700 -o deploy -g deploy /home/deploy/.ssh
printf '%s\n' "$DEPLOY_PUBKEY" > /home/deploy/.ssh/authorized_keys
chown deploy:deploy /home/deploy/.ssh/authorized_keys
chmod 600 /home/deploy/.ssh/authorized_keys

install -d -m 750 -o bunker -g bunker /var/lib/bunker /var/backups/bunker
install -d -m 750 -o root -g bunker /etc/bunker

# The secret is born here and never leaves the box. A rerun keeps the file, so
# every issued token stays valid.
if [ ! -f /etc/bunker/api.env ]; then
    cat > /etc/bunker/api.env <<ENV
APP_ENV=prod
BIND_ADDRESS=127.0.0.1
PORT=3000
DATABASE_URL=sqlite:///var/lib/bunker/bunker.db?mode=rwc
DB_MAX_CONNECTIONS=8
JWT_SECRET=$(openssl rand -hex 32)
ENV
    chown root:bunker /etc/bunker/api.env
    chmod 640 /etc/bunker/api.env
fi

install -m 644 "$here/bunker-api.service" /etc/systemd/system/bunker-api.service
install -m 644 "$here/bunker-backup.service" /etc/systemd/system/bunker-backup.service
install -m 644 "$here/bunker-backup.timer" /etc/systemd/system/bunker-backup.timer
install -m 755 "$here/bunker-backup.sh" /usr/local/sbin/bunker-backup
install -m 755 "$here/bunker-deploy.sh" /usr/local/sbin/bunker-deploy

sudoers=$(mktemp)
echo "deploy ALL=(root) NOPASSWD: /usr/local/sbin/bunker-deploy" > "$sudoers"
visudo -cf "$sudoers"
install -m 440 -o root -g root "$sudoers" /etc/sudoers.d/deploy
rm -f "$sudoers"

cat > /etc/ssh/sshd_config.d/bunker.conf <<SSHD
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin prohibit-password
AllowUsers deploy root
SSHD
systemctl enable --now ssh
systemctl restart ssh

systemctl daemon-reload
systemctl enable --now bunker-backup.timer
# The binary arrives with the first deploy, which also starts the unit.
systemctl enable bunker-api

echo "ready. a push to main ships the first binary."
