import { match } from "ts-pattern"

import type { Entrant, Tournament, TournamentStatus } from "./api"

/** Players may apply and retire only in this window. */
export const isRegistrationOpen = (t: Tournament, now = Date.now()): boolean =>
    t.status === "open" && new Date(t.registrationClosesAt).getTime() > now

/** The list comes newest event first, so the soonest open one is the last open item. */
export const nextOpen = (items: readonly Tournament[]): Tournament | undefined =>
    items.findLast(t => t.status === "open")

export type SkillLevelInfo = {
    level: 1 | 2 | 3 | 4 | 5
    name: string
    blurb: string
}

/**
 * The five levels a player picks from when they apply. The bracket seeds by
 * them: neighbours on this scale meet in round one. The names are for the
 * player; the API only ever sees the number.
 */
export const SKILL_LEVELS: readonly SkillLevelInfo[] = [
    { level: 1, name: "ROOKIE", blurb: "first time on this game. here for the vibes" },
    { level: 2, name: "CASUAL", blurb: "played a bit. knows which way is forward" },
    { level: 3, name: "REGULAR", blurb: "knows the maps. wins some, loses some" },
    { level: 4, name: "SHARP", blurb: "wins more than loses. people notice" },
    { level: 5, name: "MENACE", blurb: "sweaty. everyone knows. do not lie" },
]

/** The level the bracket gives an entrant who never picked one, and the level the apply form starts at. */
export const UNRATED_SKILL = 3

export const skillMeter = (level: number | null | undefined): string =>
    level === null || level === undefined
        ? "-----"
        : "▮".repeat(level) + "▯".repeat(Math.max(0, 5 - level))

/** A deleted account leaves the entry in place without a player. */
export const entrantName = (entrant: Entrant | undefined): string =>
    entrant?.player?.handle ?? "[deleted]"

export const STATUS_LABEL: Record<TournamentStatus, string> = {
    draft: "DRAFT",
    open: "REGISTRATION OPEN",
    live: "LIVE",
    concluded: "CONCLUDED",
}

export type BadgeTone = "accent" | "warn" | "muted" | "alert"

/** The badge of a card. An open tournament past its deadline says so. */
export const statusBadge = (
    t: Tournament,
    open = isRegistrationOpen(t),
): { label: string; tone: BadgeTone; filled: boolean } =>
    match(t.status)
        .with("open", () =>
            open
                ? { label: STATUS_LABEL.open, tone: "accent" as const, filled: true }
                : {
                      label: "REGISTRATION CLOSED",
                      tone: "muted" as const,
                      filled: false,
                  },
        )
        .with("live", () => ({
            label: STATUS_LABEL.live,
            tone: "warn" as const,
            filled: false,
        }))
        .with("concluded", () => ({
            label: STATUS_LABEL.concluded,
            tone: "muted" as const,
            filled: false,
        }))
        .with("draft", () => ({
            label: STATUS_LABEL.draft,
            tone: "muted" as const,
            filled: false,
        }))
        .exhaustive()

/** The last rounds have names everyone knows. Earlier ones get a number. */
export const roundLabel = (index: number, total: number): string =>
    match(total - index)
        .with(1, () => "final")
        .with(2, () => "semi-finals")
        .with(3, () => "quarter-finals")
        .otherwise(() => `round ${index + 1}`)
