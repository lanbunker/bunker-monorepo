#!/usr/bin/env bash
# Fills the local database with players, cycles over every rank, one open
# tournament to click through, and one event whose doors are open now, so the
# check-in page can be tried. Needs the API up (make run or make dev). Safe to
# run again: existing handles are skipped, a player with cycles keeps them, and
# neither the Seed Cup nor the Seed Night is created twice.
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

# Cycles from one adjustment each, so the leaderboard shows every rank: the
# floors are 80, 600, 1500, 3000 and 6000. Amounts stay under the 10 000 cap
# of one adjustment. A player who already has cycles is left alone.
amounts=(9400 7200 4500 3300 2100 1600 900 650 300 120)
granted=0
for i in "${!amounts[@]}"; do
    handle=${handles[$i]:-}
    [ -n "$handle" ] || break
    cycles=$(curl -fsS "$API_URL/api/players/$handle" | jq -r .standing.cycles)
    [ "$cycles" = 0 ] || continue
    player=$(curl -fsS "$API_URL/api/players/$handle" | jq -r .id)
    curl -fsS -o /dev/null -X POST "$API_URL/api/admin/players/$player/cycles" \
        -H 'content-type: application/json' -H "authorization: Bearer $admin" \
        -d "{\"amount\":${amounts[$i]},\"note\":\"seed: past seasons\"}"
    granted=$((granted + 1))
done
echo "cycles: $granted players granted, from ${amounts[0]} down to ${amounts[${#amounts[@]}-1]}"

# Past nights for the archive, then one night with its doors open now, for the
# check-in page. Rome time, 21:00 to 03:30. A night that exists by name is not
# created twice.
night() {
    name=$1 games=$2 image=$3 starts_at=$4 ends_at=$5
    existing=$(curl -fsS "$API_URL/api/events" \
        | jq -r --arg name "$name" '[.items[] | select(.name == $name)][0].id // empty')
    [ -z "$existing" ] || return 0
    id=$(curl -fsS -X POST "$API_URL/api/admin/events" \
        -H 'content-type: application/json' -H "authorization: Bearer $admin" \
        -d "{\"name\":\"$name\",\"location\":\"@theoffice\",\"games\":\"$games\",\"image\":$image,\"startsAt\":\"$starts_at\",\"endsAt\":\"$ends_at\"}" \
        | jq -r .id)
    curl -fsS -o /dev/null -X POST "$API_URL/api/admin/events/$id/status" \
        -H 'content-type: application/json' -H "authorization: Bearer $admin" \
        -d '{"status":"published"}'
    echo "event $id: $name, published"
}
night "FPS ARENA LAN PARTY" "Quake 3 Arena" '"jun2025-cover.webp"' 2025-06-21T19:00:00Z 2025-06-22T01:30:00Z
night "BUNKER//SESSION 02" "Call of Duty BO2, Mario Kart 8, casual games" '"oct2025-cover.webp"' 2025-10-28T20:00:00Z 2025-10-29T02:30:00Z
night "BUNKER//SESSION 03" "Call of Duty BO2, Halo 3, Mario Kart 8, casual games" '"feb2026-cover.webp"' 2026-02-17T20:00:00Z 2026-02-18T02:30:00Z
night "BUNKER//SESSION 04" "Call of Duty BO2, Call of Duty MW2, Halo 3, Mario Kart, arcade & casual games" null 2026-10-24T19:00:00Z 2026-10-25T02:30:00Z

open_night=$(curl -fsS "$API_URL/api/events" \
    | jq -r '[.items[] | select(.name == "Seed Night")][0].id // empty')
if [ -z "$open_night" ]; then
    starts_at=$(date -u -v-1H +%Y-%m-%dT%H:00:00Z 2>/dev/null || date -u -d '-1 hour' +%Y-%m-%dT%H:00:00Z)
    ends_at=$(date -u -v+6H +%Y-%m-%dT%H:00:00Z 2>/dev/null || date -u -d '+6 hours' +%Y-%m-%dT%H:00:00Z)
    open_night=$(curl -fsS -X POST "$API_URL/api/admin/events" \
        -H 'content-type: application/json' -H "authorization: Bearer $admin" \
        -d "{\"name\":\"Seed Night\",\"location\":\"@theoffice\",\"games\":\"Halo 3, Mario Kart 8, casual games\",\"description\":\"Seeded for development. The doors are open now.\",\"image\":\"feb2026-cover.webp\",\"startsAt\":\"$starts_at\",\"endsAt\":\"$ends_at\"}" \
        | jq -r .id)
    curl -fsS -o /dev/null -X POST "$API_URL/api/admin/events/$open_night/status" \
        -H 'content-type: application/json' -H "authorization: Bearer $admin" \
        -d '{"status":"published"}'
fi
code=$(curl -fsS "$API_URL/api/admin/events/$open_night" -H "authorization: Bearer $admin" | jq -r .checkinCode)
echo "event $open_night: Seed Night, published, doors open now. check-in at /checkin/$code"

open_cup=$(curl -fsS "$API_URL/api/tournaments" \
    | jq -r '[.items[] | select(.name == "Seed Cup" and .status == "open")][0].id // empty')
if [ -n "$open_cup" ]; then
    echo "tournament $open_cup: Seed Cup is already open, none added"
    echo "admin: seedadmin / $PASSWORD, backoffice at /admin/tournaments/$open_cup"
    exit 0
fi

closes_at=$(date -u -v+30d +%Y-%m-%dT20:00:00Z 2>/dev/null || date -u -d '+30 days' +%Y-%m-%dT20:00:00Z)
day=${closes_at%%T*}
tournament=$(curl -fsS -X POST "$API_URL/api/admin/tournaments" \
    -H 'content-type: application/json' -H "authorization: Bearer $admin" \
    -d "{\"name\":\"Seed Cup\",\"game\":\"COD Modern Warfare 2\",\"mode\":\"1v1 sniper only\",\"description\":\"Seeded for development. Rust map, one life.\",\"date\":\"$day\",\"registrationClosesAt\":\"$closes_at\"}" \
    | jq -r .id)

# A level from 1 to 5 for each entrant, so the seeded bracket has something to
# work with.
for handle in "${handles[@]:0:$ENTRANTS}"; do
    player=$(curl -fsS "$API_URL/api/players/$handle" | jq -r .id)
    skill=$((RANDOM % 5 + 1))
    curl -fsS -o /dev/null -X POST "$API_URL/api/admin/tournaments/$tournament/entrants" \
        -H 'content-type: application/json' -H "authorization: Bearer $admin" \
        -d "{\"playerId\":\"$player\",\"skill\":$skill}"
done
curl -fsS -o /dev/null -X POST "$API_URL/api/admin/tournaments/$tournament/status" \
    -H 'content-type: application/json' -H "authorization: Bearer $admin" \
    -d '{"status":"open"}'

echo "tournament $tournament: Seed Cup, open, $ENTRANTS entrants, no bracket"
echo "admin: seedadmin / $PASSWORD, backoffice at /admin/tournaments/$tournament"
