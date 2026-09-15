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

export const pad2 = (n: number): string => String(n).padStart(2, "0")

/** Format a Date as "24 OCT 2026" using the local calendar. */
export const formatDate = (date: Date): string =>
    `${pad2(date.getDate())} ${MONTHS[date.getMonth()] ?? "???"} ${date.getFullYear()}`

/** Format an ISO date such as "2026-10-24" as "24 OCT 2026", in local terms. */
export const formatIsoDay = (day: string): string =>
    formatDate(new Date(`${day}T00:00:00`))

/** `datetime-local` wants `YYYY-MM-DDTHH:mm` in the viewer's zone. */
export const toLocalInput = (iso: string): string => {
    const at = new Date(iso)
    return `${at.getFullYear()}-${pad2(at.getMonth() + 1)}-${pad2(at.getDate())}T${pad2(at.getHours())}:${pad2(at.getMinutes())}`
}
