import { match } from "ts-pattern"

import type { Entrant, Tournament, TournamentStatus } from "./api"

/** Players may apply and retire only in this window. */
export const isRegistrationOpen = (t: Tournament, now = Date.now()): boolean =>
    t.status === "open" && new Date(t.registrationClosesAt).getTime() > now

/** The list comes newest event first, so the soonest open one is the last open item. */
export const nextOpen = (items: readonly Tournament[]): Tournament | undefined =>
    items.findLast(t => t.status === "open")

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
