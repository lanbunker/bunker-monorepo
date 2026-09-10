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

const EUR_DATE = /^(\d{2})\/(\d{2})\/(\d{4})$/

const pad2 = (n: number): string => String(n).padStart(2, "0")

/** Parse DD/MM/YYYY into a local Date. Throws on a malformed or impossible date. */
export const parseEurDateStrict = (date: string): Date => {
    const match = EUR_DATE.exec(date)
    if (!match) {
        throw new Error(`Invalid date "${date}". Expected DD/MM/YYYY.`)
    }
    const [day, month, year] = match.slice(1).map(Number)
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
    date === "TBA" ? Infinity : parseEurDateStrict(date).getTime()

/** Format a Date as "24 OCT 2026" using the local calendar. */
export const formatDate = (date: Date): string =>
    `${pad2(date.getDate())} ${MONTHS[date.getMonth()]} ${date.getFullYear()}`

/** Format DD/MM/YYYY as "24 OCT 2026". TBA passes through unchanged. */
export const formatEurDate = (date: string): string =>
    date === "TBA" ? "TBA" : formatDate(parseEurDateStrict(date))
