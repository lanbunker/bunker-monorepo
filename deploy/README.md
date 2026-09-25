# Deploy the API

The API runs as one static binary in a Debian 12 LXC container on the Proxmox
host in the office. Nothing on the office network is open to the internet.
`cloudflared` in the container dials out to Cloudflare, and Cloudflare routes
two names through that tunnel:

| Name | Inside the container | Who uses it |
| --- | --- | --- |
| `api.lanbunker.eu` | `127.0.0.1:3000` | the Worker that renders the site |
| `ssh.lanbunker.eu` | `127.0.0.1:22` | GitHub Actions, key only |

GitHub Actions builds the binary, copies it over SSH through the tunnel, runs
`bunker-deploy` on the box, and checks `/health/ready` and the commit in
`/health/live` on the public name.

## What is in this directory

| File | Purpose |
| --- | --- |
| `bootstrap.sh` | Prepares a fresh container. Run one time as root. |
| `bunker-api.service` | The systemd unit. Runs as the `bunker` user. |
| `bunker-deploy.sh` | Copies the database, installs an uploaded binary, restarts the unit, and rolls back when the new binary is not ready. The `deploy` user can run only this with sudo. |
| `bunker-backup.sh` and its `.service` and `.timer` | A checked copy of the database every night at 04:00. The last 14 copies stay. |

## One-time setup

Do the steps in this order. Steps 1 and 2 are manual. Step 3 runs the script.

### 1. Create the container on Proxmox

An LXC disk is a normal volume on Proxmox storage. It is not ephemeral. It
survives stop, reboot and host upgrades, and only **Destroy** removes it. Two
volumes keep the database apart from the OS, so the data can be resized, moved
or snapshotted alone.

| Volume | Mount | Size | Holds |
| --- | --- | --- | --- |
| rootfs | `/` | 8 GB | Debian, cloudflared, the binary |
| mp0 | `/var/lib/bunker` | 4 GB | the database and its WAL |

Use the ZFS pool when the host has one. `local-lvm` works too.

1. Node > **local** > CT Templates > Templates > download `debian-12-standard`.
2. **Create CT**, tab by tab:
   - General: hostname `bunker-api`, **Unprivileged container** on, Nesting off, your SSH public key, a root password for the console.
   - Template: `debian-12-standard`.
   - Disks: the storage, size 8.
   - CPU: 1 core. Memory: 512 MB, swap 512 MB.
   - Network: bridge `vmbr0`, IPv4 DHCP. Nothing connects in, so the address does not matter.
   - Confirm: **Start after created**.
3. Container > **Resources** > Add > **Mount Point**: same storage, size 4, path `/var/lib/bunker`, Backup on.
4. Container > **Options** > Start at boot: Yes.
5. Reboot the container so the mount point is active. The bootstrap stops when `/var/lib/bunker` is not a mount point.
6. Container > **Snapshots** > Take Snapshot. A rollback undoes a failed bootstrap in seconds.

**Host backups.** Datacenter > **Backup** > Add: this container, daily, mode
Snapshot, a storage other than the one with the rootfs. Keep 7. A restore
brings back everything, tunnel credentials included, on any Proxmox host.

### 2. Create the tunnel in Cloudflare

1. Open `one.dash.cloudflare.com`. On the first visit pick a team name and the Free plan.
2. **Networks > Tunnels > Create a tunnel > Cloudflared**. Name it `bunker`. Save.
3. The install page shows commands per OS. Pick Debian 64-bit. Copy only the long string after `--token`. That is `TUNNEL_TOKEN`. Do not run the command, the bootstrap does.
4. Next, **Route tunnel**. Add the first public hostname: subdomain `api`, domain `lanbunker.eu`, type **HTTP**, URL `localhost:3000`. Save.
5. Open the tunnel > **Public Hostname** > Add: subdomain `ssh`, domain `lanbunker.eu`, type **SSH**, URL `localhost:22`. Save.

Cloudflare writes both CNAME records into the zone. The tunnel reads
**Inactive** until `cloudflared` runs in the container, then **Healthy**.

The SSH name speaks only the tunnel protocol, so a plain `ssh` client cannot
reach it. A client needs `cloudflared`, and then sshd accepts the deploy key
and nothing else. Cloudflare Access would add a token check in front, but it
needs a Zero Trust plan with a card on file, so this setup goes without it.

### 3. Make the deploy key and run the bootstrap

On your Mac, make a key pair only for GitHub Actions, then copy this directory
and the public key into the container over the LAN. The script reads the public
key from `/root/deploy_key.pub` on its own:

```bash
ssh-keygen -t ed25519 -N "" -C github-deploy -f deploy_key
scp -r deploy deploy_key.pub root@<container-ip>:/root/
```

The address is under the container > **Summary** in Proxmox. Then open a shell
in the container and run the script. It asks for the two values:

```bash
ssh -t root@<container-ip> bash /root/deploy/bootstrap.sh
```

`-t` gives the script a terminal for its one question: the **tunnel token** from
step 2. The paste stays hidden. A second run does not ask for the token, because
the cloudflared unit already holds it. It asks for the public key only when
`/root/deploy_key.pub` is gone. It refreshes everything else in place.

The script installs `cloudflared` with the token and makes its unit readable
by root only. It creates the `bunker` and `deploy` users, writes
`/etc/bunker/api.env` with a fresh `JWT_SECRET`, and creates an empty database
file. The API opens the database with `mode=rw`, which never creates a file, so
a missing volume stops the API instead of starting it on an empty database. The
script installs the units, turns on `unattended-upgrades` for Debian security
fixes, and locks SSH to keys only. The deploy key gets the `restrict` option,
and an sshd `Match User deploy` block turns off forwarding and the terminal for
that user. It prints `ready` at the end.
Within a minute the tunnel in the dashboard reads **Healthy**. The API unit is
enabled but has no binary yet. The first deploy starts it.

`deploy_key`, the private file, goes into GitHub in the next step. Delete both
key files from your Mac after that.

### 4. Set the GitHub values

In the repository, **Settings > Secrets and variables > Actions**.

| Kind | Name | Value |
| --- | --- | --- |
| secret | `DEPLOY_SSH_KEY` | the content of `deploy_key` |
| secret | `CLOUDFLARE_API_TOKEN` | a Cloudflare API token that can edit Workers |
| secret | `CLOUDFLARE_ACCOUNT_ID` | the Cloudflare account ID |
| variable | `API_HOST` | `api.lanbunker.eu` |
| variable | `SSH_HOST` | `ssh.lanbunker.eu` |
| variable | `SSH_KNOWN_HOSTS` | the host key line of the container, see below |

Put the values in the environment `production` (**Settings > Environments**).
Both deploy jobs use it. Add a rule that allows only the `main` branch.

`SSH_KNOWN_HOSTS` lets the runner check the host key of the container. Without
it, the runner trusts the first key it sees and prints a warning. Make the line
in the container:

```bash
printf 'ssh.lanbunker.eu %s\n' "$(cut -d' ' -f1,2 /etc/ssh/ssh_host_ed25519_key.pub)"
```

The name at the start must be the value of `SSH_HOST`.

### 5. First deploy

Push to `main`. The `deploy-api` job runs after `rust`, `web` and `e2e` are
green, and `deploy-web` runs after `deploy-api`. It stops early with a clear message when a GitHub value is missing. When it ends,
`https://api.lanbunker.eu/health/ready` answers `{"status":"ok",...}` and the
homepage shows `[  OK ] bunkernet uplink`.

## Deploy and rollback

`bunker-deploy` does these steps:

1. It refuses an upload that is a symlink, has more than one link, or does not
   belong to `deploy`. It moves the upload into `/var/lib/bunker-deploy`, which
   only root reads.
2. It stops the API and copies the database to
   `/var/backups/bunker/pre-deploy.db`.
3. It keeps the running binary as `/usr/local/bin/bunker-api.prev` and installs
   the new one.
4. It starts the API. The API applies the new migrations at startup.
5. When `/health/ready` does not answer in 15 seconds, it puts back
   `bunker-api.prev` and `pre-deploy.db` and starts the old binary. The job
   fails in both cases. The writes that the new binary made are lost.

A manual rollback after a deploy that passed its checks:

```bash
systemctl stop bunker-api
install -m 755 /usr/local/bin/bunker-api.prev /usr/local/bin/bunker-api
systemctl start bunker-api
```

The old binary refuses a database with a migration it does not know. Then
restore `pre-deploy.db` as in **Restore a backup** below. That loses every write
after the deploy.

## Day-to-day

- **Logs**: `journalctl -u bunker-api -f` in the container.
- **Make an admin**: `runuser -u bunker -- sqlite3 -cmd '.timeout 5000' /var/lib/bunker/bunker.db "update players set role = 'admin' where handle = 'dave' collate nocase"`. Run it as `bunker`, because a WAL file that root creates can block the API.
- **Restore a backup**: see below.
- **Rotate the deploy key**: make a new pair, put the new public key in `/root/deploy_key.pub`, and rerun the bootstrap. It writes `authorized_keys` with the `restrict` option. Replace the `DEPLOY_SSH_KEY` secret.
- **Update the units or scripts**: copy `deploy/` again with `scp -r` and rerun the bootstrap. It restarts the API when the API runs.
- **The tunnel token** sits in `/etc/systemd/system/cloudflared.service`. Only root reads it.
- **Move the container**: Proxmox backup and restore keeps everything, including the tunnel credentials. Nothing to change in Cloudflare or GitHub.

### Restore a backup

The nightly copies are `/var/backups/bunker/bunker-<date>.db.gz`. The copy from
the last deploy is `/var/backups/bunker/pre-deploy.db`, not compressed.

```bash
systemctl stop bunker-api
runuser -u bunker -- sh -c 'gunzip -c /var/backups/bunker/bunker-<date>.db.gz > /var/lib/bunker/restore.db'
runuser -u bunker -- sqlite3 /var/lib/bunker/restore.db 'pragma integrity_check'
rm -f /var/lib/bunker/bunker.db-wal /var/lib/bunker/bunker.db-shm
mv /var/lib/bunker/restore.db /var/lib/bunker/bunker.db
chown bunker:bunker /var/lib/bunker/bunker.db
chmod 600 /var/lib/bunker/bunker.db
systemctl start bunker-api
```

The integrity check must print `ok`. If it prints anything else, stop and use
an older copy. For `pre-deploy.db`, use `cp` instead of `gunzip -c`. Remove the
WAL and the SHM files before the move. An old WAL on top of a restored file
damages the database.

## What is not covered

- No staging. `main` is production. The `dev` branch runs CI but deploys nothing.
- The Proxmox host itself: keep its own backups of the container with vzdump.

## Remaining before public launch

- An off-site backup. Copy `/var/backups/bunker` to Cloudflare R2 every night with `rclone` or `restic`.
- A Cloudflare WAF rate-limiting rule on `api.lanbunker.eu/api/auth/*`, and on `/_actions/login` and `/_actions/signup` on the site.
- Lock `api.lanbunker.eu` so only the Worker can call it.
- An external uptime check on `https://api.lanbunker.eu/health/ready`.
- Pin each GitHub action in `.github/workflows/ci.yml` to a commit SHA.
- Create the GitHub environment `production`, move the deploy secrets and variables into it, and allow only `main`.
