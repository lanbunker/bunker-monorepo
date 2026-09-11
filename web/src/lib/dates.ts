const MONTHS = [
    "JAN",
    "FEB",
    "MAR",
    "APR",
    "MAY",
    "JUN",
    "JUL",
    "AUG",
    "SEP",
    "OCT",
    "NOV",
    "DEC",
] as const

const EUR_DATE = /^(?<day>\d{2})\/(?<month>\d{2})\/(?<year>\d{4})$/

/** The value the content schema uses for a date that is not fixed yet. */
export const TBA = "TBA"

export const pad2 = (n: number): string => String(n).padStart(2, "0")

/** Parse DD/MM/YYYY into a local Date. Throws on a malformed or impossible date. */
export const parseEurDateStrict = (date: string): Date => {
    const groups = EUR_DATE.exec(date)?.groups
    if (!groups) {
        throw new Error(`Invalid date "${date}". Expected DD/MM/YYYY.`)
    }
    const day = Number(groups.day)
    const month = Number(groups.month)
    const year = Number(groups.year)
    const parsed = new Date(year, month - 1, day)
    const roundTrips =
        parsed.getFullYear() === year &&
        parsed.getMonth() === month - 1 &&
        parsed.getDate() === day
    if (!roundTrips) {
        throw new Error(`Invalid date "${date}". The day does not exist in that month.`)
    }
    return parsed
}

/** Parse DD/MM/YYYY to timestamp for sorting. TBA sorts to top (Infinity). */
export const parseEurDate = (date: string): number =>
    date === TBA ? Infinity : parseEurDateStrict(date).getTime()

/** Format a Date as "24 OCT 2026" using the local calendar. */
export const formatDate = (date: Date): string =>
    `${pad2(date.getDate())} ${MONTHS[date.getMonth()] ?? "???"} ${date.getFullYear()}`

/** Format DD/MM/YYYY as "24 OCT 2026". TBA passes through unchanged. */
export const formatEurDate = (date: string): string =>
    date === TBA ? TBA : formatDate(parseEurDateStrict(date))

/** Format an ISO date such as "2026-10-24" as "24 OCT 2026", in local terms. */
export const formatIsoDay = (day: string): string =>
    formatDate(new Date(`${day}T00:00:00`))

/** `datetime-local` wants `YYYY-MM-DDTHH:mm` in the viewer's zone. */
export const toLocalInput = (iso: string): string => {
    const at = new Date(iso)
    return `${at.getFullYear()}-${pad2(at.getMonth() + 1)}-${pad2(at.getDate())}T${pad2(at.getHours())}:${pad2(at.getMinutes())}`
}
