#!/usr/bin/env bash
# Run the DB-backed test suite against a real Postgres.
#
# Picks, in order: $TEST_DATABASE_URL if set (its database name must end in
# _test; the suite creates and drops per-test databases next to it, so the
# user needs CREATEDB); else a throwaway `docker run postgres:18` on an
# ephemeral loopback port; else, when not root, a throwaway initdb cluster
# under $TMPDIR. Prints SKIPPED and exits 0 only when none of those work.
#
# In the limes sandbox initdb cannot run (uid 0) and the postgres image cannot
# drop privileges under the sandbox's uid mapping, so .limes.local.toml
# exports TEST_DATABASE_URL pointing at the koryto_test
# database of the scratch Postgres started on the host by scripts/scratch-pg.sh.
# A set but unreachable URL is a failure, not a skip: start the scratch server.
# Extra arguments go to `cargo test`.
set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE=${TEST_PG_IMAGE:-postgres:18}
cleanup=()
trap 'for c in "${cleanup[@]:-}"; do [ -n "$c" ] && eval "$c" >/dev/null 2>&1 || true; done' EXIT

if [ -n "${TEST_DATABASE_URL:-}" ]; then
  case "${TEST_DATABASE_URL%%\?*}" in
    *_test) ;;
    *) echo "test-db: refusing TEST_DATABASE_URL: database name must end in _test" >&2; exit 1 ;;
  esac
  if ! psql "$TEST_DATABASE_URL" -Atc 'select 1' >/dev/null 2>&1; then
    echo "test-db: TEST_DATABASE_URL is set but unreachable; on the host run scripts/scratch-pg.sh up" >&2
    exit 1
  fi
  echo "test-db: using TEST_DATABASE_URL"
elif docker info >/dev/null 2>&1; then
  name="koryto-test-pg-$$-$RANDOM"
  cid=$(docker run -d --rm --name "$name" -e POSTGRES_PASSWORD=test -e POSTGRES_DB=koryto_test \
        -p 127.0.0.1:0:5432 "$IMAGE")
  cleanup+=("docker rm -f $cid")
  port=$(docker port "$cid" 5432/tcp | head -1 | sed 's/.*://')
  for _ in $(seq 1 60); do
    docker exec "$cid" pg_isready -U postgres -d koryto_test >/dev/null 2>&1 && break
    sleep 1
  done
  docker exec "$cid" pg_isready -U postgres -d koryto_test >/dev/null
  export TEST_DATABASE_URL="postgres://postgres:test@127.0.0.1:$port/koryto_test"
  echo "test-db: docker $IMAGE on port $port"
elif [ "$(id -u)" != 0 ] && command -v initdb >/dev/null 2>&1; then
  dir=$(mktemp -d "${TMPDIR:-/tmp}/koryto-test-pg.XXXXXX")
  cleanup+=("pg_ctl -D $dir/data stop -m immediate; rm -rf $dir")
  initdb -D "$dir/data" -A trust -U postgres --no-locale -E UTF8 >/dev/null
  pg_ctl -D "$dir/data" -o "-k $dir -h '' -c listen_addresses=''" -l "$dir/log" start >/dev/null
  createdb -h "$dir" -U postgres koryto_test
  export TEST_DATABASE_URL="postgres://postgres@localhost/koryto_test?host=$dir"
  echo "test-db: initdb cluster in $dir"
else
  echo "SKIPPED: no TEST_DATABASE_URL, no docker, and initdb cannot run as root"
  exit 0
fi

cargo test "$@"
