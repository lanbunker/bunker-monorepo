#!/usr/bin/env bash
# Fills the local database with players and one tournament to click through.
# Needs the API up (make run or make dev). Safe to run again: existing handles
# are skipped, and a new tournament is added each time.
#
#   make seed
set -euo pipefail

API_URL=${API_URL:-http://127.0.0.1:3000}
DB_FILE=${DB_FILE:-.dev/bunker.db}
PASSWORD=${SEED_PASSWORD:-bunker-seed-pass}
PLAYERS=${SEED_PLAYERS:-30}
ENTRANTS=${SEED_ENTRANTS:-23}

# Thirty accounts with a published password belong on a laptop and nowhere else.
case "$API_URL" in
    http://127.0.0.1:*|http://localhost:*) ;;
    *) echo "refusing to seed $API_URL: only a local api" >&2; exit 1 ;;
esac
[ -f "$DB_FILE" ] || { echo "no local database at $DB_FILE" >&2; exit 1; }

curl -fsS "$API_URL/health/ready" >/dev/null \
    || { echo "the api does not answer at $API_URL. start it with make run" >&2; exit 1; }

signup() {
    curl -sS -o /dev/null -w '%{http_code}' -X POST "$API_URL/api/auth/signup" \
        -H 'content-type: application/json' \
        -d "{\"handle\":\"$1\",\"password\":\"$PASSWORD\"}"
}

login() {
    curl -fsS -X POST "$API_URL/api/auth/login" \
        -H 'content-type: application/json' \
        -d "{\"handle\":\"$1\",\"password\":\"$PASSWORD\"}" | jq -r .token
}

# The admin exists after the first run, so 409 is fine here.
status=$(signup seedadmin)
[ "$status" = 201 ] || [ "$status" = 409 ] || { echo "signup of seedadmin answered $status" >&2; exit 1; }
sqlite3 "$DB_FILE" "update players set role = 'admin' where handle = 'seedadmin'"
admin=$(login seedadmin)

handles=()
for i in $(seq 1 "$PLAYERS"); do
    handle=$(printf 'player%02d' "$i")
    status=$(signup "$handle")
    [ "$status" = 201 ] || [ "$status" = 409 ] || { echo "signup of $handle answered $status" >&2; exit 1; }
    handles+=("$handle")
done
echo "$PLAYERS players: player01 .. $(printf 'player%02d' "$PLAYERS"), password $PASSWORD"

closes_at=$(date -u -v+30d +%Y-%m-%dT20:00:00Z 2>/dev/null || date -u -d '+30 days' +%Y-%m-%dT20:00:00Z)
day=${closes_at%%T*}
tournament=$(curl -fsS -X POST "$API_URL/api/admin/tournaments" \
    -H 'content-type: application/json' -H "authorization: Bearer $admin" \
    -d "{\"name\":\"Seed Cup\",\"game\":\"COD Modern Warfare 2\",\"mode\":\"1v1 sniper only\",\"description\":\"Seeded for development. Rust map, one life.\",\"date\":\"$day\",\"registrationClosesAt\":\"$closes_at\"}" \
    | jq -r .id)

for handle in "${handles[@]:0:$ENTRANTS}"; do
    player=$(curl -fsS "$API_URL/api/players/$handle" | jq -r .id)
    curl -fsS -o /dev/null -X POST "$API_URL/api/admin/tournaments/$tournament/entrants" \
        -H 'content-type: application/json' -H "authorization: Bearer $admin" \
        -d "{\"playerId\":\"$player\"}"
done
curl -fsS -o /dev/null -X POST "$API_URL/api/admin/tournaments/$tournament/status" \
    -H 'content-type: application/json' -H "authorization: Bearer $admin" \
    -d '{"status":"open"}'

echo "tournament $tournament: Seed Cup, open, $ENTRANTS entrants, no bracket"
echo "admin: seedadmin / $PASSWORD, backoffice at /admin/tournaments/$tournament"
