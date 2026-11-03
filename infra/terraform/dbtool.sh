#!/bin/sh
# Runs inside public.ecr.aws/docker/library/postgres:16-alpine as a one-off ECS task
# (aws_ecs_task_definition.dbtool), driven by scripts/aws-hibernate.sh.
#   MODE=backup  : pg_dump to /tmp/db.dump, trial-restore it into a scratch database on the same server,
#                  then PUT it to URL (a presigned S3 URL).
#   MODE=restore : GET URL to /tmp/db.dump and pg_restore it into DATABASE_URL. Skipped when the database
#                  already has tables, unless FORCE=1.
# Every status line starts with "dbtool:" so the script can pick them out of the CloudWatch log stream.
set -eu
: "${MODE:?MODE=backup|restore}" "${URL:?URL=presigned S3 URL}" "${DATABASE_URL:?}"
apk add --no-cache curl >/dev/null 2>&1 || { echo "dbtool: apk add curl failed"; exit 1; }
DUMP=/tmp/db.dump
TEST_DB=wartide_restore_test
TEST_URL="${DATABASE_URL%/*}/$TEST_DB"
users() { psql "$1" -Atc "select count(*) from users" 2>/dev/null || echo "n/a"; }
case "$MODE" in
  backup)
    pg_dump --no-owner --no-privileges -Fc -f "$DUMP" "$DATABASE_URL"
    echo "dbtool: dumped tables=$(pg_restore --list "$DUMP" | grep -c ' TABLE DATA ') users=$(users "$DATABASE_URL")"
    psql "$DATABASE_URL" -qc "DROP DATABASE IF EXISTS $TEST_DB" -c "CREATE DATABASE $TEST_DB"
    pg_restore --no-owner --no-privileges --exit-on-error -d "$TEST_URL" "$DUMP"
    echo "dbtool: restore-test ok users=$(users "$TEST_URL")"
    psql "$DATABASE_URL" -qc "DROP DATABASE $TEST_DB"
    echo "dbtool: bytes=$(stat -c %s "$DUMP") md5=$(md5sum "$DUMP" | cut -d' ' -f1)"
    curl -fsS -X PUT -T "$DUMP" "$URL"
    echo "dbtool: uploaded"
    ;;
  restore)
    n=$(psql "$DATABASE_URL" -Atc "select count(*) from pg_tables where schemaname='public'")
    if [ "$n" != 0 ] && [ "${FORCE:-0}" != 1 ]; then
      echo "dbtool: skip (database already has $n tables; run with FORCE=1 to overwrite)"
      exit 0
    fi
    curl -fsS -o "$DUMP" "$URL"
    echo "dbtool: downloaded bytes=$(stat -c %s "$DUMP") md5=$(md5sum "$DUMP" | cut -d' ' -f1)"
    pg_restore --no-owner --no-privileges --clean --if-exists --exit-on-error -d "$DATABASE_URL" "$DUMP"
    echo "dbtool: restored users=$(users "$DATABASE_URL")"
    ;;
  *) echo "dbtool: unknown MODE '$MODE'"; exit 2 ;;
esac
