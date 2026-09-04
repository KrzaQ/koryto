#!/usr/bin/env bash
# Scratch PostgreSQL for development and sandbox tests, run FROM THE HOST
# with the host's docker.
#
# Publishes on the wired address so a limes sandbox can reach it (the sandbox
# sees the host at <host>, never at localhost). Two databases: koryto_scratch
# for `make dev-backend` and psql poking, and koryto_test as the maintenance
# database for scripts/test-db.sh (the suite creates and drops per-test
# databases next to it). Data lives under ./data/scratch-pg so `down --purge`
# is a clean slate.
#
#   scripts/scratch-pg.sh up        start (idempotent) and print the URLs
#   scripts/scratch-pg.sh down      stop and remove the container, keep data
#   scripts/scratch-pg.sh down --purge   ... and delete ./data/scratch-pg
#   scripts/scratch-pg.sh psql      open psql inside the container
#   scripts/scratch-pg.sh url       print the connection URLs
set -euo pipefail

NAME=${SCRATCH_PG_NAME:-koryto-scratch-pg}
BIND=${SCRATCH_PG_BIND:-<host>}
PORT=${SCRATCH_PG_PORT:-15434}
USER_=${SCRATCH_PG_USER:-koryto}
PASS=${SCRATCH_PG_PASSWORD:-scratch}
DB=${SCRATCH_PG_DB:-koryto_scratch}
TEST_DB=${SCRATCH_PG_TEST_DB:-koryto_test}
IMAGE=${SCRATCH_PG_IMAGE:-postgres:18}
DIR=$(cd "$(dirname "$0")/.." && pwd)/data/scratch-pg

url() { echo "postgres://$USER_:$PASS@$BIND:$PORT/$1"; }

case ${1:-} in
  up)
    mkdir -p "$DIR"
    if docker ps -a --format '{{.Names}}' | grep -qx "$NAME"; then
      docker start "$NAME" >/dev/null
    else
      # postgres:18 keeps its data one level below /var/lib/postgresql.
      docker run -d --name "$NAME" --restart unless-stopped \
        -e POSTGRES_USER="$USER_" -e POSTGRES_PASSWORD="$PASS" -e POSTGRES_DB="$DB" \
        -p "$BIND:$PORT:5432" \
        -v "$DIR:/var/lib/postgresql" \
        "$IMAGE" >/dev/null
    fi
    for _ in $(seq 1 30); do
      docker exec "$NAME" pg_isready -U "$USER_" -d "$DB" >/dev/null 2>&1 && break
      sleep 1
    done
    docker exec "$NAME" pg_isready -U "$USER_" -d "$DB"
    # The test maintenance database; idempotent.
    docker exec "$NAME" psql -U "$USER_" -d "$DB" -Atc \
      "select 1 from pg_database where datname = '$TEST_DB'" | grep -qx 1 ||
      docker exec "$NAME" createdb -U "$USER_" "$TEST_DB"
    echo "KORYTO_DATABASE_URL=$(url "$DB")"
    echo "TEST_DATABASE_URL=$(url "$TEST_DB")"
    ;;
  down)
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    if [ "${2:-}" = "--purge" ]; then
      # The image writes as uid 999; sudo only if plain rm cannot.
      rm -rf "$DIR" 2>/dev/null || sudo rm -rf "$DIR"
    fi
    ;;
  psql)
    docker exec -it "$NAME" psql -U "$USER_" -d "${2:-$DB}"
    ;;
  url)
    url "$DB"
    url "$TEST_DB"
    ;;
  *)
    sed -n '2,17p' "$0"
    exit 2
    ;;
esac
