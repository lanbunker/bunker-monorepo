#!/usr/bin/env bash
# Prepares a Debian 12 container for the API. Run it as root inside the
# container, from a terminal:
#
#   ssh -t root@<container-ip> bash /root/deploy/bootstrap.sh
#
# It asks only for what the box does not hold yet: the tunnel token, the deploy
# public key (read from /root/deploy_key.pub when present) and the Backblaze B2
# values. Each can also come from the environment: TUNNEL_TOKEN, DEPLOY_PUBKEY,
# B2_KEY_ID, B2_APP_KEY, B2_BUCKET, B2_ENDPOINT.
#
# A rerun keeps the tunnel, the secrets and the database, installs the units and
# the scripts again, and restarts what runs.
set -euo pipefail

readonly HERE=$(cd "$(dirname "$0")" && pwd)
readonly DATA=/var/lib/bunker
readonly DB=$DATA/bunker.db
readonly API_ENV=/etc/bunker/api.env
readonly LITESTREAM_ENV=/etc/bunker/litestream.env
readonly LITESTREAM_CONFIG=/etc/bunker/litestream.yml
readonly CLOUDFLARED_UNIT=/etc/systemd/system/cloudflared.service
readonly LITESTREAM_VERSION=0.5.17
readonly LITESTREAM_SHA256=cfb371176d164437ae869f8351cfde49bd1804ae71c61923f75c9cba9c9c006d

fail() {
    echo "bootstrap: $*" >&2
    exit 1
}

# `read -p` shows nothing without a terminal, so `ssh host cmd` would wait in
# silence.
ask() {
    local prompt=$1 var=$2 hidden=${3:-}
    [ -t 0 ] || fail "no terminal to ask for the $prompt. Run it with 'ssh -t', or set $var."
    if [ -n "$hidden" ]; then
        read -r -s -p "$prompt: " "$var"
        echo
    else
        read -r -p "$prompt: " "$var"
    fi
    [ -n "${!var}" ] || fail "the $prompt is empty"
}

as_bunker_with_b2() {
    local env_file=$1
    shift
    runuser -u bunker -- sh -c 'set -a && . "$0" && exec "$@"' "$env_file" "$@"
}

check_volume() {
    # Without the volume, the database would sit on the rootfs, and a new
    # container would lose it.
    mountpoint -q "$DATA" || fail "$DATA is not a mount point. Add the mp0 volume first."
}

collect_inputs() {
    if [ ! -f "$CLOUDFLARED_UNIT" ] && [ -z "${TUNNEL_TOKEN:-}" ]; then
        ask "tunnel token" TUNNEL_TOKEN hidden
    fi

    if [ -z "${DEPLOY_PUBKEY:-}" ] && [ -f /root/deploy_key.pub ]; then
        DEPLOY_PUBKEY=$(cat /root/deploy_key.pub)
    fi
    [ -n "${DEPLOY_PUBKEY:-}" ] || ask "deploy public key" DEPLOY_PUBKEY
    case "$DEPLOY_PUBKEY" in
        ssh-ed25519\ * | ssh-rsa\ *) ;;
        *) fail "the deploy public key must start with ssh-ed25519 or ssh-rsa" ;;
    esac

    if [ ! -f "$LITESTREAM_ENV" ]; then
        [ -n "${B2_KEY_ID:-}" ] || ask "B2 keyID" B2_KEY_ID
        [ -n "${B2_APP_KEY:-}" ] || ask "B2 applicationKey" B2_APP_KEY hidden
        [ -n "${B2_BUCKET:-}" ] || ask "B2 bucket name" B2_BUCKET
        [ -n "${B2_ENDPOINT:-}" ] || ask "B2 endpoint" B2_ENDPOINT
        B2_ENDPOINT=${B2_ENDPOINT#https://}
        case "$B2_ENDPOINT" in
            s3.*.backblazeb2.com) ;;
            *) fail "the B2 endpoint must look like s3.eu-central-003.backblazeb2.com" ;;
        esac
    fi
}

install_packages() {
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -q
    apt-get install -y -q --no-install-recommends \
        ca-certificates curl openssh-server openssl sqlite3 sudo unattended-upgrades
    cat > /etc/apt/apt.conf.d/20auto-upgrades <<'APT'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
APT
}

install_cloudflared() {
    if ! command -v cloudflared >/dev/null; then
        install -d -m 755 /usr/share/keyrings
        curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg \
            -o /usr/share/keyrings/cloudflare-main.gpg
        echo "deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared any main" \
            > /etc/apt/sources.list.d/cloudflared.list
        apt-get update -q
        apt-get install -y -q cloudflared
    fi
    [ -f "$CLOUDFLARED_UNIT" ] || cloudflared service install "$TUNNEL_TOKEN"
    # The unit file holds the tunnel token.
    chmod 600 "$CLOUDFLARED_UNIT"
}

install_litestream() {
    local current
    current=$(litestream version 2>/dev/null || true)
    [ "${current#v}" = "$LITESTREAM_VERSION" ] && return

    local tarball
    tarball=$(mktemp)
    curl -fsSL -o "$tarball" \
        "https://github.com/benbjohnson/litestream/releases/download/v$LITESTREAM_VERSION/litestream-$LITESTREAM_VERSION-linux-x86_64.tar.gz"
    echo "$LITESTREAM_SHA256  $tarball" | sha256sum -c --quiet
    tar -xzf "$tarball" -C /usr/local/bin --no-same-owner litestream
    chmod 755 /usr/local/bin/litestream
    rm -f "$tarball"
}

create_users() {
    # `bunker` runs the API and owns the data. `deploy` is the account GitHub
    # Actions logs into, and sudo lets it run one script.
    id -u bunker >/dev/null 2>&1 \
        || useradd --system --home-dir "$DATA" --shell /usr/sbin/nologin bunker
    id -u deploy >/dev/null 2>&1 \
        || useradd --create-home --shell /bin/bash deploy

    install -d -m 700 -o deploy -g deploy /home/deploy/.ssh
    install -m 600 -o deploy -g deploy /dev/null /home/deploy/.ssh/authorized_keys
    printf 'restrict %s\n' "$DEPLOY_PUBKEY" > /home/deploy/.ssh/authorized_keys

    install -d -m 750 -o bunker -g bunker "$DATA" /var/backups/bunker
    install -d -m 750 -o root -g bunker /etc/bunker
    install -d -m 700 -o root -g root /var/lib/bunker-deploy
}

write_api_env() {
    # The JWT secret is made here and never leaves the box. A rerun keeps it,
    # so every issued token stays valid.
    if [ ! -f "$API_ENV" ]; then
        install -m 640 -o root -g bunker /dev/null "$API_ENV"
        cat > "$API_ENV" <<ENV
APP_ENV=prod
BIND_ADDRESS=127.0.0.1
PORT=3000
DATABASE_URL=sqlite://$DB?mode=rw
DB_MAX_CONNECTIONS=8
JWT_SECRET=$(openssl rand -hex 32)
ENV
    fi
    # `mode=rw` never creates the file, so a missing volume stops the API
    # instead of starting it on an empty database.
    sed -i 's|^\(DATABASE_URL=.*\)?mode=rwc$|\1?mode=rw|' "$API_ENV"
}

# A typo in a B2 value would break every backup without a sound, so the values
# are saved only after B2 accepts them. On a new disk the database comes back
# from B2 in the same step.
setup_replica() {
    install -m 644 "$HERE/litestream.yml" "$LITESTREAM_CONFIG"

    local env_file=$LITESTREAM_ENV
    if [ ! -f "$LITESTREAM_ENV" ]; then
        env_file=$LITESTREAM_ENV.new
        install -m 640 -o root -g bunker /dev/null "$env_file"
        cat > "$env_file" <<ENV
LITESTREAM_ACCESS_KEY_ID=$B2_KEY_ID
LITESTREAM_SECRET_ACCESS_KEY=$B2_APP_KEY
B2_BUCKET=$B2_BUCKET
B2_ENDPOINT=$B2_ENDPOINT
B2_REGION=$(echo "$B2_ENDPOINT" | cut -d. -f2)
ENV
    fi

    if [ ! -e "$DB" ]; then
        echo "no database on the volume. Restoring it from B2 when B2 holds one."
        as_bunker_with_b2 "$env_file" litestream restore -config "$LITESTREAM_CONFIG" \
            -if-replica-exists -integrity-check full "$DB" \
            || fail "B2 refused the restore. Check the B2 values and run again."
        [ -e "$DB" ] || install -m 600 -o bunker -g bunker /dev/null "$DB"
        chmod 600 "$DB"
    elif [ "$env_file" != "$LITESTREAM_ENV" ]; then
        local check
        check=$(runuser -u bunker -- mktemp -d)
        as_bunker_with_b2 "$env_file" litestream restore -config "$LITESTREAM_CONFIG" \
            -if-replica-exists -o "$check/bunker.db" "$DB" >/dev/null \
            || { rm -rf "$check"; fail "B2 refused the values. Check them and run again."; }
        rm -rf "$check"
    fi

    [ "$env_file" = "$LITESTREAM_ENV" ] || mv "$env_file" "$LITESTREAM_ENV"
}

install_units() {
    install -m 644 "$HERE/bunker-api.service" "$HERE/bunker-backup.service" \
        "$HERE/bunker-backup.timer" "$HERE/litestream.service" /etc/systemd/system/
    install -m 755 "$HERE/bunker-backup.sh" /usr/local/sbin/bunker-backup
    install -m 755 "$HERE/bunker-deploy.sh" /usr/local/sbin/bunker-deploy
}

configure_sudo() {
    local sudoers
    sudoers=$(mktemp)
    echo 'deploy ALL=(root) NOPASSWD: /usr/local/sbin/bunker-deploy ""' > "$sudoers"
    visudo -cf "$sudoers" >/dev/null
    install -m 440 -o root -g root "$sudoers" /etc/sudoers.d/deploy
    rm -f "$sudoers"
}

configure_sshd() {
    local conf=/etc/ssh/sshd_config.d/bunker.conf
    local prev
    prev=$(mktemp)
    [ ! -f "$conf" ] || cp "$conf" "$prev"
    cat > "$conf" <<'SSHD'
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin prohibit-password
AllowUsers deploy root

Match User deploy
    AllowTcpForwarding no
    X11Forwarding no
    PermitTTY no
SSHD
    # A broken config would lock every SSH login out after the restart.
    if ! sshd -t; then
        if [ -s "$prev" ]; then cp "$prev" "$conf"; else rm -f "$conf"; fi
        rm -f "$prev"
        fail "sshd refused the new config. The old one is back."
    fi
    rm -f "$prev"
    systemctl enable ssh
    systemctl restart ssh
}

start_services() {
    systemctl daemon-reload
    systemctl enable --now bunker-backup.timer
    systemctl enable litestream
    systemctl restart litestream
    # The first deploy brings the binary and starts the API.
    systemctl enable bunker-api
    systemctl try-restart bunker-api
}

check_volume
collect_inputs
install_packages
install_cloudflared
install_litestream
create_users
write_api_env
setup_replica
install_units
configure_sudo
configure_sshd
start_services
echo "bootstrap: ready. A push to main ships the first binary."
