#!/usr/bin/env sh
set -eu

YETII_CONFIG="${YETII_CONFIG:-/etc/yetii/yetii.yaml}"

mkdir -p /var/lib/yetii /var/log/yetii

if [ "$#" -eq 0 ]; then
  set -- daemon start
fi

case "$1" in
  yetii|/usr/local/bin/yetii)
    exec "$@"
    ;;
  --file|-c|--help|-h|--version|-V)
    exec yetii "$@"
    ;;
  init|odbc|setup|run|check-config|daemon)
    exec yetii --file "$YETII_CONFIG" "$@"
    ;;
  *)
    exec "$@"
    ;;
esac
