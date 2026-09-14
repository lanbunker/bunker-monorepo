/** Rows per page on every list. The API allows up to 100. */
export const PAGE_SIZE = 20

/** Lines of the cycles log a profile shows before it offers the full log. */
export const RECENT_ENTRIES = 5

/** The longest handle. A longer term matches nothing, and the API refuses it. */
export const SEARCH_MAX = 20

/**
 * Reads `?q=` from a URL. Blank is no search, so a form that submits an empty
 * field lists everyone, and a longer term is cut to the length of a handle.
 */
export const searchFrom = (url: URL): string | undefined => {
    const term = (url.searchParams.get("q") ?? "").trim().slice(0, SEARCH_MAX)
    return term === "" ? undefined : term
}

/** A list URL with its search and, past the first page, its page. */
export const listPath = (href: string, q: string | undefined, page: number): string => {
    const params = new URLSearchParams([
        ...(q === undefined ? [] : [["q", q]]),
        ...(page > 1 ? [["page", String(page)]] : []),
    ])
    const query = params.toString()
    return query === "" ? href : `${href}?${query}`
}

/**
 * Reads `?page=` from a URL. Anything that is not a positive integer becomes page
 * 1, so a bad link shows the first page instead of an error.
 */
export const pageFrom = (url: URL): number => {
    const raw = Number(url.searchParams.get("page"))
    return Number.isInteger(raw) && raw >= 1 ? raw : 1
}
