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
`bunker-deploy` on the box, and checks `/health/ready` on the public name.

## What is in this directory

| File | Purpose |
| --- | --- |
| `bootstrap.sh` | Prepares a fresh container. Run one time as root. |
| `bunker-api.service` | The systemd unit. Runs as the `bunker` user. |
| `bunker-deploy.sh` | Installs an uploaded binary and restarts the unit. The `deploy` user can run only this with sudo. |
| `bunker-backup.sh` and its `.service` and `.timer` | A consistent copy of the database every night at 04:00, kept 14 days. |

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
5. Reboot the container so the mount point is active.
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
step 2. The paste stays hidden. A second run asks nothing, because `cloudflared`
is already installed, and refreshes everything else in place.

The script installs `cloudflared` with the token, creates the `bunker` and
`deploy` users, writes `/etc/bunker/api.env` with a fresh `JWT_SECRET`,
installs the units, and locks SSH to keys only. It prints `ready` at the end.
Within a minute the tunnel in the dashboard reads **Healthy**. The API unit is
enabled but has no binary yet. The first deploy starts it.

`deploy_key`, the private file, goes into GitHub in the next step. Delete both
key files from your Mac after that.

### 4. Set the GitHub values

In the repository, **Settings > Secrets and variables > Actions**.

| Kind | Name | Value |
| --- | --- | --- |
| secret | `DEPLOY_SSH_KEY` | the content of `deploy_key` |
| variable | `API_HOST` | `api.lanbunker.eu` |
| variable | `SSH_HOST` | `ssh.lanbunker.eu` |

### 5. First deploy

Push to `main`. The `deploy-api` job runs after `rust`, `web` and `e2e` are
green. It stops early with a clear message when a GitHub value is missing. When it ends,
`https://api.lanbunker.eu/health/ready` answers `{"status":"ok",...}` and the
homepage shows `[  OK ] bunkernet uplink`.

## Day-to-day

- **Logs**: `journalctl -u bunker-api -f` in the container.
- **Make an admin**: `sqlite3 /var/lib/bunker/bunker.db "update players set role = 'admin' where handle = 'dave' collate nocase"`.
- **Restore a backup**: stop the unit, `gunzip -c /var/backups/bunker/bunker-<date>.db.gz > /var/lib/bunker/bunker.db`, `chown bunker:bunker` the file, start the unit.
- **Rotate the deploy key**: make a new pair, replace `/home/deploy/.ssh/authorized_keys`, replace the `DEPLOY_SSH_KEY` secret.
- **Update the units or scripts**: copy `deploy/` again with `scp -r` and rerun the bootstrap. It asks nothing the second time.
- **The tunnel token** sits in `/etc/systemd/system/cloudflared.service`, readable by the local accounts. Both are system accounts under your control.
- **Move the container**: Proxmox backup and restore keeps everything, including the tunnel credentials. Nothing to change in Cloudflare or GitHub.

## What is not covered

- No staging. `main` is production. The `dev` branch runs CI but deploys nothing.
- No rate limit on the API. Add a Cloudflare WAF rate-limiting rule on `api.lanbunker.eu/api/auth/*` when signups open to the public.
- The Proxmox host itself: keep its own backups of the container with vzdump.
