import type { Entrant, Tournament, TournamentStatus } from "./api"

/** Players may apply and retire only in this window. */
export const isRegistrationOpen = (t: Tournament, now = Date.now()): boolean =>
    t.status === "open" && new Date(t.registrationClosesAt).getTime() > now

/** The list comes newest event first, so the soonest open one is the last open item. */
export const nextOpen = (items: Tournament[]): Tournament | undefined =>
    items.filter(t => t.status === "open").at(-1)

/** A deleted account leaves the entry in place without a player. */
export const entrantName = (entrant: Entrant | undefined): string =>
    entrant?.player?.handle ?? "[deleted]"

export const STATUS_LABEL: Record<TournamentStatus, string> = {
    draft: "DRAFT",
    open: "REGISTRATION OPEN",
    live: "LIVE",
    concluded: "CONCLUDED",
}

/** The last rounds have names everyone knows. Earlier ones get a number. */
export const roundLabel = (index: number, total: number): string => {
    const fromEnd = total - index
    if (fromEnd === 1) return "final"
    if (fromEnd === 2) return "semi-finals"
    if (fromEnd === 3) return "quarter-finals"
    return `round ${index + 1}`
}

/** `datetime-local` wants `YYYY-MM-DDTHH:mm` in the viewer's zone. */
export const toLocalInput = (iso: string): string => {
    const d = new Date(iso)
    const pad = (n: number) => String(n).padStart(2, "0")
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}
