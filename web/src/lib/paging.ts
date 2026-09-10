/** Rows per page on every list. The API allows up to 100. */
export const PAGE_SIZE = 20

/**
 * Reads `?page=` from a URL. Anything that is not a positive integer becomes page
 * 1, so a bad link shows the first page instead of an error.
 */
export const pageFrom = (url: URL): number => {
    const raw = Number(url.searchParams.get("page"))
    return Number.isInteger(raw) && raw >= 1 ? raw : 1
}
