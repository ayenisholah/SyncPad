# Deploying SyncPad

SyncPad runs as a **single container** behind nginx (in-memory documents pin a
document to one process — spec §12). The container listens on port 8090 bound to
host loopback; nginx reverse-proxies the public subdomain to it. These steps
target Ubuntu; TLS is added at the end.

Delivery is two GitHub Actions workflows:

- **Container Image** (`.github/workflows/container-image.yml`) builds the image
  and publishes it to the **GitHub Container Registry**
  (`ghcr.io/ayenisholah/syncpad`) on every push to `main` (tag `edge`), on `v*`
  tags, and on demand.
- **Deploy Production** (`.github/workflows/deploy-production.yml`) is a manual,
  one-click deploy: you pick a published image tag, and it SSHes to the VPS,
  ships the `deploy/` bundle, and runs `docker compose pull` + `up -d`. Nothing
  deploys automatically on push.

## Prerequisites (once, on the Ubuntu VPS)

```sh
# Docker Engine + the Compose plugin (official convenience script):
curl -fsSL https://get.docker.com | sudo sh

# nginx (reverse proxy). Skip if it is already installed.
sudo apt-get update && sudo apt-get install -y nginx

# certbot for TLS, added later:
sudo apt-get install -y certbot python3-certbot-nginx
```

Verify: `docker --version`, `docker compose version`, `nginx -v`. You also need
a DNS **A record** for `syncpad.sholaayeni.xyz` pointing at the VPS's public IP.
The Deploy Production workflow ships the compose file to `/opt/syncpad`, so the
VPS does **not** need a clone of the repository.

## 1. Generate the deploy key and load the environment secrets

Deploy authenticates to the VPS with a dedicated SSH key held in a GitHub
**Environment** (`production`). Set it up once.

**a. Generate a CI-only ed25519 keypair** (on your machine — never commit it):

```sh
ssh-keygen -t ed25519 -C 'syncpad-deploy' -f ~/.ssh/syncpad_deploy -N ''
```

**b. Authorize the public key on the VPS** (one password prompt):

```sh
ssh -o StrictHostKeyChecking=accept-new root@43.106.12.32 \
  "install -m 700 -d ~/.ssh && cat >> ~/.ssh/authorized_keys" < ~/.ssh/syncpad_deploy.pub
```

**c. Capture the VPS host key** for `VPS_KNOWN_HOSTS` (deploy uses
`StrictHostKeyChecking=yes`, so the host key must be pinned):

```sh
ssh-keyscan -t ed25519 43.106.12.32
```

**d. Create the environment and add its secrets.** In GitHub: **Settings ▸
Environments ▸ New environment** → name it `production` → **Add secret** for each
row:

| Secret | Value |
|---|---|
| `VPS_HOST` | `43.106.12.32` |
| `VPS_USER` | `root` |
| `VPS_SSH_KEY` | the **private** key — the full contents of `~/.ssh/syncpad_deploy` (including the `-----BEGIN/END-----` lines) |
| `VPS_KNOWN_HOSTS` | the line printed by `ssh-keyscan` in step (c) |

## 2. First run

1. Push to `main` (or run **Container Image** manually). The workflow builds and
   pushes `ghcr.io/ayenisholah/syncpad:edge`.
2. In the repository's **Packages**, open the `syncpad` package → **Package
   settings** → set visibility to **Public** so the VPS pulls without a login.
3. Actions ▸ **Deploy Production** ▸ **Run workflow**, leave `image_tag` as
   `edge`. It ships the bundle, pulls the image, starts the container on
   `127.0.0.1:8090` with the `syncpad-data` volume for `/data`, and runs a smoke
   check (`POST /api/docs`).

Every later deploy is the same one click — pick `edge`, a `v*` tag, or a
`sha-…` tag. **Rollback** = run Deploy Production again with an older tag.

## 3. Wire up nginx (once, on the VPS — HTTP first)

The bundle lands at `/opt/syncpad/deploy` after the first deploy:

```sh
sudo cp /opt/syncpad/deploy/nginx.txt /etc/nginx/sites-available/syncpad
sudo ln -s /etc/nginx/sites-available/syncpad /etc/nginx/sites-enabled/syncpad
sudo nginx -t && sudo systemctl reload nginx
```

`nginx.txt` contains a `map $http_upgrade $connection_upgrade { … }` block that
must live in the **http { }** context. If your nginx already defines
`$connection_upgrade` (a common shared snippet, e.g. from another project on the
host), delete that map from the copied file to avoid a duplicate. If your
`nginx.conf` includes `conf.d/*` rather than `sites-enabled/*`, copy the file to
`/etc/nginx/conf.d/syncpad.conf` instead.

Visit `http://syncpad.sholaayeni.xyz` — the editor should load. Open the same
document link in two browsers and type: the text converges.

## 4. TLS (when ready)

```sh
sudo certbot --nginx -d syncpad.sholaayeni.xyz
```

certbot adds the `listen 443` / certificate lines and an HTTP→HTTPS redirect.
The client switches to `wss://` automatically once the page is served over
HTTPS — no app change needed.

## Operations

On the VPS, compose lives at `/opt/syncpad/deploy/docker-compose.yml`.

- **Update / deploy a version:** Actions ▸ **Deploy Production** ▸ Run workflow
  (pick the tag). No SSH needed.
- **Rollback:** run Deploy Production again with an older `edge`/`v*`/`sha-…`
  tag.
- **Graceful stop:** `docker compose -f /opt/syncpad/deploy/docker-compose.yml
  stop` sends SIGTERM; the server flushes dirty snapshots before exiting (up to
  the 10 s grace period).
- **Logs:** `docker compose -f /opt/syncpad/deploy/docker-compose.yml logs -f`.
- **Data:** `/data` (the `syncpad-data` volume) holds per-document JSON
  snapshots. It is disposable by policy — documents expire after 24 h idle — and
  only needs to survive restarts.
- **Measure latency and capacity:** Actions > **Measure Production** > Run
  workflow. The public job measures the real HTTPS/WSS path; the capacity job
  runs the same harness on the VPS loopback interface and uses synthetic source
  addresses so the intentional 10-documents-per-public-IP limit remains
  enabled. Download both JSONL artifacts and record reviewed results in
  `docs/measurements.md`.
- **Manual deploy (no Actions):** with the package public,
  `SYNCPAD_IMAGE=ghcr.io/ayenisholah/syncpad:edge docker compose -f
  /opt/syncpad/deploy/docker-compose.yml pull && docker compose -f
  /opt/syncpad/deploy/docker-compose.yml up -d`.
