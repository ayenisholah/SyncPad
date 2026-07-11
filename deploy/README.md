# Deploying SyncPad

SyncPad runs as a **single container** behind nginx (in-memory documents pin a
document to one process — spec §12). The container listens on port 8090 bound to
host loopback; nginx reverse-proxies the public subdomain to it. These steps
target Ubuntu; TLS is added at the end.

## Prerequisites (install on the Ubuntu VPS)

```sh
# Docker Engine + the Compose plugin (official convenience script):
curl -fsSL https://get.docker.com | sudo sh
# Let your user run docker without sudo (log out/in afterwards):
sudo usermod -aG docker "$USER"

# nginx (reverse proxy). Skip if it is already installed.
sudo apt-get update && sudo apt-get install -y nginx

# certbot for TLS, added later:
sudo apt-get install -y certbot python3-certbot-nginx
```

Verify: `docker --version`, `docker compose version`, `nginx -v`.

You also need the repository on the VPS (`git clone …`) and a DNS **A record**
for `syncpad.sholaayeni.xyz` pointing at the VPS's public IP.

## 1. Build and run the container

From the repository root:

```sh
docker compose -f deploy/docker-compose.yml up -d --build
```

This builds the image (frontend + server) and starts SyncPad on
`127.0.0.1:8090` with a named volume `syncpad-data` for `/data` (the snapshots).
Check it is up:

```sh
docker compose -f deploy/docker-compose.yml ps
curl -s -X POST http://127.0.0.1:8090/api/docs   # → {"docId":"…"}
```

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

- **Update to a new version:** `git pull` then
  `docker compose -f deploy/docker-compose.yml up -d --build`.
- **Graceful stop:** `docker compose -f deploy/docker-compose.yml stop` sends
  SIGTERM; the server flushes dirty snapshots before exiting (up to the 10 s
  grace period).
- **Logs:** `docker compose -f deploy/docker-compose.yml logs -f`.
- **Data:** `/data` (the `syncpad-data` volume) holds per-document JSON
  snapshots. It is disposable by policy — documents expire after 24 h idle — and
  only needs to survive restarts.
