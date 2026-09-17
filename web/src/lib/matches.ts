import type { MatchRecord, PlayedMatch } from "./api"
import { roundLabel } from "./tournaments"

/** `14-9`: wins first, always from the view of the profile owner. */
export const recordLine = (record: MatchRecord): string =>
    `${record.wins}-${record.losses}`

export const played = (record: MatchRecord): number => record.wins + record.losses

/** The percent of matches won, or nothing when no match was played. */
export const winRate = (record: MatchRecord): number | undefined => {
    const total = played(record)
    return total === 0 ? undefined : Math.round((record.wins / total) * 100)
}

/** The path a prompt prints: the owner reads their own file. */
export const matchesPath = (handle: string, own: boolean): string =>
    own ? "~/.matches" : `/players/${handle}/matches`

/** `final`, `semi-finals` or a number. The API counts rounds from 1. */
export const matchRound = (match: PlayedMatch): string =>
    roundLabel(match.round - 1, match.rounds)
