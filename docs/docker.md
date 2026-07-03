# Docker deployment

Yetii can run in a container with the CLI binary, unixODBC, and common ODBC drivers installed.

The image is designed for immutable production use: install drivers at image build time, then use `setup --check-only` at runtime to verify the YAML-selected drivers are present.

## Build

```bash
docker build -t yetii:local .
```

## Pull published image

The GitHub Actions Docker workflow publishes release images to GitHub Container Registry:

```bash
docker pull ghcr.io/<github-owner>/yetii:latest
```

Use the published image the same way as the local image:

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  -v yetii-state:/var/lib/yetii \
  -e ERP_PASSWORD=secret \
  -e API_TOKEN=secret \
  ghcr.io/<github-owner>/yetii:latest run
```

The workflow uses:

```text
ghcr.io/${{ github.repository_owner }}/yetii
```

Publish behavior:

- pull requests: build and smoke-test only
- `main`: publish `main` and `sha-*` tags
- `v*` tags: publish version, major/minor, `latest`, and `sha-*` tags

Workflow file:

```text
.github/workflows/docker.yml
```

Publishing requires GitHub Actions package write permission:

```text
Settings → Actions → General → Workflow permissions → Read and write permissions
```

## Validate configuration

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  -e ERP_PASSWORD=secret \
  -e API_TOKEN=secret \
  -e OPS_API_TOKEN=secret \
  yetii:local check-config
```

## Verify ODBC drivers

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  yetii:local setup --check-only
```

This command reads the configured databases, resolves the exact ODBC driver names Yetii will use, and checks the registered driver list.

It exits non-zero if a required driver is missing.

## One-shot run

Run one query:

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  -v yetii-state:/var/lib/yetii \
  -e ERP_PASSWORD=secret \
  -e API_TOKEN=secret \
  yetii:local run --query orders_sync
```

Run all enabled queries:

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  -v yetii-state:/var/lib/yetii \
  -e ERP_PASSWORD=secret \
  -e API_TOKEN=secret \
  yetii:local run
```

## Daemon mode

The container default command is:

```bash
yetii --file /etc/yetii/yetii.yaml daemon start
```

Run it with Docker:

```bash
docker run -d \
  --name yetii \
  --restart unless-stopped \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  -v yetii-state:/var/lib/yetii \
  -e ERP_PASSWORD=secret \
  -e API_TOKEN=secret \
  -e OPS_API_TOKEN=secret \
  -p 8080:8080 \
  -p 9090:9090 \
  yetii:local
```

Do not use `daemon start --detach` inside Docker. Docker already runs the container as the detached process. Yetii should stay in the foreground so Docker can manage logs, restart policy, and shutdown signals.

## Health, metrics, and logs

```bash
curl -fsS http://localhost:8080/health
curl -fsS http://localhost:9090/metrics
docker logs -f yetii
docker stop yetii
```

## Config path

The entrypoint defaults to:

```text
/etc/yetii/yetii.yaml
```

Override it with `YETII_CONFIG`:

```bash
docker run --rm \
  -v ./prod.yaml:/app/config/prod.yaml:ro \
  -e YETII_CONFIG=/app/config/prod.yaml \
  yetii:local check-config
```

Or pass `--file` explicitly:

```bash
docker run --rm \
  -v ./prod.yaml:/app/config/prod.yaml:ro \
  yetii:local --file /app/config/prod.yaml check-config
```

## Secrets

Use environment variables, Docker secrets, or your orchestrator's secret mechanism. Do not bake secrets into the image.

Example with environment variables:

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  -e ERP_PASSWORD=secret \
  -e API_TOKEN=secret \
  -e OPS_API_TOKEN=secret \
  yetii:local check-config
```

The YAML can reference them:

```yaml
auth:
  username: erp_user
  password: ${ERP_PASSWORD}
```

## State volume

Incremental sync requires persistent state. Mount `/var/lib/yetii` or configure `execution.state_management.state_file` to point inside a mounted volume.

Recommended:

```yaml
execution:
  state_management:
    enabled: true
    state_file: /var/lib/yetii/yetii_state.json
    backup_states: 5
```

Docker volume:

```bash
-v yetii-state:/var/lib/yetii
```

Without a persistent volume, incremental watermarks are lost when the container is removed.

## Docker Compose smoke test

The repository includes a local end-to-end proof stack:

```text
examples/docker-compose/
  compose.yaml
  yetii.yaml
  init.sql
  mock-receiver/
```

It starts:

```text
Postgres container → Yetii container over ODBC → mock HTTP receiver container
```

Run it:

```bash
cd examples/docker-compose
docker compose up --build --abort-on-container-exit --exit-code-from yetii
```

Expected result:

- Postgres starts and seeds the `orders` table.
- Yetii waits for Postgres and the receiver to become healthy.
- Yetii runs `orders_sync` once.
- Yetii reads rows through ODBC.
- Yetii POSTs the batch to `mock-receiver`.
- Yetii exits successfully.

Inspect the receiver logs:

```bash
docker compose logs mock-receiver
```

Clean up:

```bash
docker compose down -v
```

If the first run fails while pulling `postgres:16` or `node:22-alpine`, fix Docker Hub access or DNS for Docker Desktop and rerun the command.

## Bundled drivers

The default image installs:

- unixODBC
- PostgreSQL ODBC driver from Debian packages
- MariaDB/MySQL-compatible ODBC driver from Debian packages
- a `MySQL ODBC 8.0 Unicode Driver` compatibility registration pointing at the bundled MariaDB/MySQL-compatible driver

Driver names are validated with:

```bash
docker run --rm yetii:local odbc
```

If your YAML uses a different driver name, set `databases.driver` to the exact registered name shown by `yetii odbc`.

## SQL Server, Oracle, and custom drivers

SQL Server and Oracle drivers often require vendor package repositories, license acceptance, or Instant Client files. Those are intentionally not hidden inside Yetii runtime behavior.

For those databases, build a custom image that starts from `yetii:local` and installs/registers the vendor driver:

```dockerfile
FROM yetii:local

# Install vendor ODBC driver packages here.
# Then verify:
# RUN odbcinst -q -d
```

Then configure the exact registered driver name:

```yaml
databases:
  name: billing
  type: mssql
  driver: ODBC Driver 18 for SQL Server
```

Run:

```bash
docker run --rm \
  -v ./yetii.yaml:/etc/yetii/yetii.yaml:ro \
  yetii-custom:local setup --check-only
```

## Cloud databases

Cloud-hosted databases work from Docker as long as:

- the container has network access to the DB host
- DNS resolves from inside the container
- the DB firewall/security group allows the container host or cluster
- TLS certificates are trusted by the container
- the configured ODBC driver supports the required TLS/auth mode

For Docker Desktop, `localhost` inside the container means the container itself, not the host. Use `host.docker.internal` for host services on Docker Desktop, or a real network hostname for cloud databases.
