# Deploy the API

The API is one static binary in a Debian 13 LXC container on the office Proxmox
box. No port is open to the internet: `cloudflared` in the container dials out,
and Cloudflare routes two names through the tunnel.

| Name | Goes to | Used by |
| --- | --- | --- |
| `api.lanbunker.eu` | `127.0.0.1:3000` | the Worker that renders the site |
| `ssh.lanbunker.eu` | `127.0.0.1:22` | GitHub Actions, with one key |

A push to `main` builds the binary, uploads it through the tunnel, and runs
`bunker-deploy` on the box.

| File | Purpose |
| --- | --- |
| `bootstrap.sh` | Sets up the container. Safe to run again. |
| `bunker-deploy.sh` | Installs an uploaded binary, and rolls back when it is not healthy. The only command the `deploy` user can run with sudo. |
| `bunker-api.service` | The API, as the `bunker` user. |
| `litestream.service`, `litestream.yml` | Sends each database change to Backblaze B2 within 10 seconds. A full copy each day, kept 7 days. |
| `bunker-backup.sh`, `.service`, `.timer` | A checked copy of the database on the box at 04:00, kept 14 days. |

## Setup

Do these steps one time, in this order.

### 1. Container

1. In Proxmox, download the `debian-13-standard` template.
2. **Create CT**: hostname `bunker-api`, unprivileged, **nesting on** (systemd in Debian 13 needs it, or journald does not start), your SSH key, 8 GB disk, 1 core, 512 MB memory, 512 MB swap, bridge `vmbr0` with DHCP, start after creation.
3. **Resources > Add > Mount Point**: 4 GB, path `/var/lib/bunker`, backup on. The database lives here, apart from the OS.
4. **Options**: start at boot.
5. Reboot the container so the mount point is active.
6. Optional: **Datacenter > Backup**, a daily snapshot of the container to another storage, keep 7.

### 2. Cloudflare tunnel

1. In `one.dash.cloudflare.com`, **Networks > Tunnels > Create a tunnel > Cloudflared**, name `bunker`.
2. On the install page, copy only the long string after `--token`. Do not run the command.
3. Add a public hostname: `api.lanbunker.eu`, type HTTP, URL `localhost:3000`.
4. Add a public hostname: `ssh.lanbunker.eu`, type SSH, URL `localhost:22`.

### 3. Backblaze B2

B2 gives 10 GB free and asks for no credit card.

1. **Buckets > Create a Bucket**: a unique name, private, object lock off.
2. In the bucket's **Lifecycle Settings**, pick **Keep only the last version of the file**.
3. Copy the bucket **Endpoint**, for example `s3.eu-central-003.backblazeb2.com`.
4. **Application Keys > Add a New Application Key**: access to this bucket only, read and write. Copy the **keyID** and the **applicationKey**.
5. Save the four values in your password manager. You need them again to rebuild a lost box.

### 4. Bootstrap

On your Mac, make a key only for GitHub Actions, and copy this directory to the
container:

```bash
ssh-keygen -t ed25519 -N "" -C github-deploy -f deploy_key
scp -r deploy deploy_key.pub root@<container-ip>:/root/
ssh -t root@<container-ip> bash /root/deploy/bootstrap.sh
```

The script asks for the tunnel token and the four B2 values. It saves the B2
values only after B2 accepts them. It installs everything, makes a JWT secret,
and prints `ready`. The tunnel then shows **Healthy** in Cloudflare.

### 5. GitHub

In **Settings > Environments**, create `production` and allow only `main`. Add:

| Kind | Name | Value |
| --- | --- | --- |
| secret | `DEPLOY_SSH_KEY` | the content of `deploy_key` |
| secret | `CLOUDFLARE_API_TOKEN` | a Cloudflare token that can edit Workers |
| secret | `CLOUDFLARE_ACCOUNT_ID` | the Cloudflare account ID |
| secret | `API_KEY` | the shared API key, made with `openssl rand -hex 32` |
| secret | `TURNSTILE_SITE_KEY` | from the Turnstile widget, see below |
| secret | `TURNSTILE_SECRET_KEY` | from the same widget |
| variable | `API_HOST` | `api.lanbunker.eu` |
| variable | `SSH_HOST` | `ssh.lanbunker.eu` |
| variable | `SSH_KNOWN_HOSTS` | the output of the command below, run in the container |

```bash
printf 'ssh.lanbunker.eu %s\n' "$(cut -d' ' -f1,2 /etc/ssh/ssh_host_ed25519_key.pub)"
```

Create the Turnstile widget in Cloudflare under **Turnstile > Add widget**, for
the site's hostname, in managed mode.

Each deploy carries `API_KEY` to the box, on stdin of `bunker-deploy`, and all
three secrets to the Worker. GitHub is the one place to change them.

Then delete `deploy_key` and `deploy_key.pub` from your Mac, and push to `main`.

## Upgrade the box to the API key

Do this one time, before the first push to `main` that adds `API_KEY`. The box
keeps the old `bunker-deploy` until `bootstrap.sh` runs again. The old script
ignores the key, and the new API refuses to start without it.

```bash
scp -r deploy root@<container-ip>:/root/
ssh -t root@<container-ip> bash /root/deploy/bootstrap.sh
```

Then add the `API_KEY` and Turnstile secrets in GitHub, and push.

## How a deploy works

`bunker-deploy` runs one deploy at a time:

1. It refuses an upload that is a symlink, or that does not belong to `deploy`.
2. It writes the API key from stdin into `/etc/bunker/api.env` when the key changed, and keeps the old file to roll back. An empty line or a terminal keeps the stored key.
3. It stops the API and copies the database to `/var/backups/bunker/pre-deploy.db`.
4. It keeps the old binary as `/usr/local/bin/bunker-api.prev`, installs the new one, and starts it. Migrations run at startup.
5. When `/health/ready` does not answer in 15 seconds, it puts back the old binary, the pre-deploy database and the old API key. The job fails.

Then CI checks that `https://api.lanbunker.eu/health/live` answers the commit it built.

Do not run `wrangler deploy` by hand from a tree where `make web-e2e` ran. The e2e build writes test keys into `web/dist`. CI builds the site again for each deploy.

## Day to day

Run these in the container as root.

| Task | Command |
| --- | --- |
| Logs | `journalctl -u bunker-api -f`, `journalctl -u litestream -f` |
| Make an admin | `runuser -u bunker -- sqlite3 -cmd '.timeout 5000' /var/lib/bunker/bunker.db "update players set role = 'admin' where handle = 'dave' collate nocase"` |
| Update the scripts or units | copy `deploy/` again, then run `bootstrap.sh` again |
| Rotate the deploy key | put the new public key in `/root/deploy_key.pub`, run `bootstrap.sh`, replace `DEPLOY_SSH_KEY` |
| Rotate the B2 key | edit `/etc/bunker/litestream.env`, then `systemctl restart litestream` |
| Rotate the API key | change `API_KEY` in GitHub, then run the last deploy again. Do this at a quiet time, see below |

Run `sqlite3` as `bunker`. A WAL file that root creates blocks the API.

A rotation of the API key stops the site for a few minutes. The API restarts
with the new key in `deploy-api`, and the Worker sends the old key until
`deploy-web` uploads the new secret. When `deploy-web` fails, the site stays
down until a deploy succeeds.

Once a month, restore the B2 copy to a scratch file. It must print a number:

```bash
runuser -u bunker -- sh -c 'set -a && . /etc/bunker/litestream.env && litestream restore -config /etc/bunker/litestream.yml -o /tmp/check.db /var/lib/bunker/bunker.db'
sqlite3 /tmp/check.db 'select count(*) from players' && rm -f /tmp/check.db
```

## Restore

| Source | Loses |
| --- | --- |
| B2 | up to 10 seconds, and nothing after a clean stop |
| `/var/backups/bunker/bunker-<date>.db.gz` | up to one day |
| `/var/backups/bunker/pre-deploy.db` | everything after the last deploy |

**The box is lost.** Do steps 1 and 4 again with the same B2 values. The
bootstrap restores the database from B2. Then push to `main`, or run the last
deploy again.

**The data is wrong.** Stop both services, make `restore.db` from one source,
and put it in place:

```bash
systemctl stop bunker-api litestream
cd /var/lib/bunker

# From B2. Add -timestamp 2026-10-05T18:00:00Z for an earlier point:
runuser -u bunker -- sh -c 'set -a && . /etc/bunker/litestream.env && litestream restore -config /etc/bunker/litestream.yml -o restore.db bunker.db'
# Or from a nightly copy:
runuser -u bunker -- sh -c 'gunzip -c /var/backups/bunker/bunker-<date>.db.gz > restore.db'

runuser -u bunker -- sqlite3 restore.db 'pragma integrity_check'   # must print ok
rm -rf bunker.db-wal bunker.db-shm .bunker.db-litestream
mv restore.db bunker.db && chown bunker:bunker bunker.db && chmod 600 bunker.db
systemctl start litestream bunker-api
```

The old WAL file must go, or it damages the restored file. The
`.bunker.db-litestream` directory must go too, so Litestream starts a new
generation in B2.

## Before a public launch

- A Cloudflare rate-limit rule on `/api/auth/*`, and on `/_actions/login` and `/_actions/signup` of the site.
- Make `api.lanbunker.eu` accept only the Worker.
- An uptime check on `https://api.lanbunker.eu/health/ready`.
- Pin each GitHub action in `ci.yml` to a commit SHA.
