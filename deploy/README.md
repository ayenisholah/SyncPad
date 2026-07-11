# Deploying SyncPad

SyncPad runs as a **single container** behind nginx (in-memory documents pin a
document to one process — spec §12). The container listens on port 8090 bound to
host loopback; nginx reverse-proxies the public subdomain to it. These steps
target Ubuntu; TLS is added at the end.

Delivery is automated: **GitHub Actions** builds the image, publishes it to the
**GitHub Container Registry** (`ghcr.io/ayenisholah/syncpad`), and — on a
version tag or a manual run — SSHes into the VPS to pull the new image and
restart the container. See `.github/workflows/deploy.yml`.

## Prerequisites (install on the Ubuntu VPS)

```sh
# Docker Engine + the Compose plugin (official convenience script):
curl -fsSL https://get.docker.com | sudo sh
# Let your user run docker without sudo (log out/in afterwards). Skip if you
# deploy as root.
sudo usermod -aG docker "$USER"

# nginx (reverse proxy). Skip if it is already installed.
sudo apt-get update && sudo apt-get install -y nginx

# certbot for TLS, added later:
sudo apt-get install -y certbot python3-certbot-nginx
```

Verify: `docker --version`, `docker compose version`, `nginx -v`.

You also need a DNS **A record** for `syncpad.sholaayeni.xyz` pointing at the
VPS's public IP, and a clone of the repository on the VPS (the deploy job runs
`git pull` here to pick up compose/nginx changes):

```sh
git clone https://github.com/ayenisholah/SyncPad.git ~/SyncPad
```

## 1. One-time CI setup

The pipeline pushes to GHCR with the built-in `GITHUB_TOKEN` (no PAT) and
deploys over SSH with a dedicated key. Configure it once:

1. **Deploy key.** Generate a CI-only ed25519 keypair (never commit it):

   ```sh
   ssh-keygen -t ed25519 -N '' -C 'syncpad-ci-deploy' -f ./syncpad_deploy
   ```

   Add the **public** key to the VPS deploy user's authorized keys:

   ```sh
   ssh-copy-id -i ./syncpad_deploy.pub root@43.106.12.32
   # or append the contents of syncpad_deploy.pub to ~/.ssh/authorized_keys
   ```

2. **Repository secrets** (GitHub → Settings → Secrets and variables → Actions):

   | Secret | Value |
   |---|---|
   | `VPS_HOST` | `43.106.12.32` |
   | `VPS_USER` | `root` |
   | `VPS_SSH_KEY` | the **private** key (contents of `syncpad_deploy`) |
   | `VPS_SSH_PORT` | `22` (or your SSH port) |

3. **First image + public package.** Push to `main` (or run the workflow) so the
   `build-push` job publishes the image, then in the GHCR package settings set
   its visibility to **Public** so the VPS pulls without a registry login.

## 2. Deploy

- **Release:** tag a commit and push the tag — this runs build → push → deploy:

  ```sh
  git tag v0.1.0 && git push origin v0.1.0
  ```

- **Manual:** Actions tab → **Deploy** workflow → **Run workflow** (deploys
  `latest`).

The deploy job runs, on the VPS: `git pull`, `docker compose -f
deploy/docker-compose.yml pull`, then `up -d` — starting SyncPad on
`127.0.0.1:8090` with a named volume `syncpad-data` for `/data` (the snapshots).
Check it is up:

```sh
docker compose -f deploy/docker-compose.yml ps
curl -s -X POST http://127.0.0.1:8090/api/docs   # → {"docId":"…"}
```

**Manual fallback** (no CI): with the repo cloned on the VPS and the package
public, `docker compose -f deploy/docker-compose.yml pull && docker compose -f
deploy/docker-compose.yml up -d` pulls and runs the latest image directly.

## 2. Wire up nginx (HTTP first)

```sh
# Server block for the subdomain:
sudo cp deploy/nginx.txt /etc/nginx/sites-available/syncpad
sudo ln -s /etc/nginx/sites-available/syncpad /etc/nginx/sites-enabled/syncpad
```

`deploy/nginx.txt` contains a `map $http_upgrade $connection_upgrade { … }`
block that must live in the **http { }** context. If your nginx already defines
`$connection_upgrade` (a common shared snippet), delete that map from the copied
file to avoid a duplicate. Then:

```sh
sudo nginx -t && sudo systemctl reload nginx
```

Visit `http://syncpad.sholaayeni.xyz` — the editor should load. Open the same
document link in two browsers and type: the text converges.

## 3. TLS (when ready)

```sh
sudo certbot --nginx -d syncpad.sholaayeni.xyz
```

certbot adds the `listen 443` / certificate lines and an HTTP→HTTPS redirect.
The client switches to `wss://` automatically once the page is served over
HTTPS — no app change needed.

## Operations

- **Update to a new version:** push a `v*` tag (or run the Deploy workflow) —
  CI rebuilds, publishes, and rolls the container.
- **Rollback:** re-run the Deploy workflow at an older tag, or on the VPS pin an
  earlier image and restart:
  `SYNCPAD_TAG=<older-sha-or-version> docker compose -f
  deploy/docker-compose.yml pull && docker compose -f deploy/docker-compose.yml
  up -d` (image tags are the git short SHA, `latest`, and each release version).
- **Graceful stop:** `docker compose -f deploy/docker-compose.yml stop` sends
  SIGTERM; the server flushes dirty snapshots before exiting (up to the 10 s
  grace period).
- **Logs:** `docker compose -f deploy/docker-compose.yml logs -f`.
- **Data:** `/data` (the `syncpad-data` volume) holds per-document JSON
  snapshots. It is disposable by policy — documents expire after 24 h idle — and
  only needs to survive restarts.
