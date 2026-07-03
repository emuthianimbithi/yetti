FROM rust:1-bookworm AS builder

WORKDIR /usr/src/yetii

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        unixodbc-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim AS runtime

LABEL org.opencontainers.image.title="Yetii" \
      org.opencontainers.image.description="ODBC-powered database-to-HTTP sync runner"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        procps \
        unixodbc \
        odbcinst \
        odbc-postgresql \
        odbc-mariadb \
    && MYSQL_ODBC_DRIVER="$(find /usr/lib -path '*/odbc/libmaodbc.so' -print -quit)" \
    && if [ -n "$MYSQL_ODBC_DRIVER" ]; then \
        { \
          printf '\n[MySQL ODBC 8.0 Unicode Driver]\n'; \
          printf 'Description=MariaDB/MySQL-compatible Unicode ODBC driver\n'; \
          printf 'Driver=%s\n' "$MYSQL_ODBC_DRIVER"; \
          printf 'Setup=%s\n' "$MYSQL_ODBC_DRIVER"; \
          printf 'UsageCount=1\n'; \
        } >> /etc/odbcinst.ini; \
      fi \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /etc/yetii /var/lib/yetii /var/log/yetii

COPY --from=builder /usr/src/yetii/target/release/YETII /usr/local/bin/yetii
COPY docker/entrypoint.sh /usr/local/bin/yetii-entrypoint

RUN chmod +x /usr/local/bin/yetii /usr/local/bin/yetii-entrypoint

ENV YETII_CONFIG=/etc/yetii/yetii.yaml

VOLUME ["/var/lib/yetii"]

EXPOSE 8080 9090

ENTRYPOINT ["yetii-entrypoint"]
CMD ["daemon", "start"]
