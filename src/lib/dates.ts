/** Parse DD/MM/YYYY to timestamp for sorting. TBA sorts to top (Infinity). */
export function parseEurDate(date: string): number {
    if (date === "TBA") return Infinity
    const [day, month, year] = date.split("/")
    return new Date(Number(year), Number(month) - 1, Number(day)).getTime()
}
