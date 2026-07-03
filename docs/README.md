# Yetii documentation

Start with the main [README](../README.md) for installation, command usage, configuration overview, and operational guidance.

Reference docs:

- [Docker deployment](docker.md): container usage, GHCR images, Compose smoke stack, state volumes, bundled drivers, custom driver images, and cloud database notes.
- [Incremental synchronization](incremental-sync.md): scalar and composite watermarks, page execution, cursor correctness rules, and database-specific SQL examples.
- [Notification services](notifications.md): pluggable HTTP notifications, event names, auth, payload templates, and legacy webhook compatibility.

Examples:

- [Docker Compose smoke stack](../examples/docker-compose/compose.yaml): local end-to-end proof using Postgres, Yetii, and a mock HTTP receiver.
