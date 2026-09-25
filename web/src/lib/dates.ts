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

/**
 * The zone of the venue. The Worker runs in UTC, so a date the server renders
 * names this zone, and an evening entry lands on the day the players saw.
 */
export const SITE_TIME_ZONE = "Europe/Rome"

export const pad2 = (n: number): string => String(n).padStart(2, "0")

/** The calendar and clock fields of an instant in a zone, each as two or four digits. */
const partsIn = (date: Date, timeZone: string) => {
    const parts = new Intl.DateTimeFormat("en-GB", {
        timeZone,
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        hourCycle: "h23",
    }).formatToParts(date)
    const part = (type: Intl.DateTimeFormatPartTypes): string =>
        parts.find(candidate => candidate.type === type)?.value ?? "00"
    return {
        year: part("year"),
        month: part("month"),
        day: part("day"),
        hour: part("hour"),
        minute: part("minute"),
    }
}

const monthName = (month: string): string => MONTHS[Number(month) - 1] ?? "???"

/** Format a Date as "24 OCT 2026" in the zone of the venue. */
export const formatDate = (date: Date): string => {
    const parts = partsIn(date, SITE_TIME_ZONE)
    return `${parts.day} ${monthName(parts.month)} ${parts.year}`
}

/** Format an ISO date such as "2026-10-24" as "24 OCT 2026". A day has no zone. */
export const formatIsoDay = (day: string): string => {
    const [year = "????", month = "", date = "??"] = day.split("-")
    return `${date} ${monthName(month)} ${year}`
}

/**
 * `datetime-local` wants `YYYY-MM-DDTHH:mm` in one zone. The server renders the
 * venue zone, and the browser replaces it with the zone of the viewer.
 */
export const toLocalInput = (iso: string, timeZone: string): string => {
    const parts = partsIn(new Date(iso), timeZone)
    return `${parts.year}-${parts.month}-${parts.day}T${parts.hour}:${parts.minute}`
}
