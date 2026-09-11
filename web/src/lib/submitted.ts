/** What a form posted, minus anything secret. Empty for every other request. */
export type Submitted = Readonly<Record<string, string>>

const EMPTY: Submitted = {}

const FORM_TYPES = ["application/x-www-form-urlencoded", "multipart/form-data"]

/**
 * The largest body worth reading twice. Every form on the site posts a few
 * hundred bytes, the longest field being a description of 1000 characters. A
 * body over this is not a form of ours, so it is left alone.
 */
const MAX_BYTES = 64 * 1024

/** A field whose value must never come back to the page or the log. */
const isSecret = (name: string): boolean =>
    /password|token|secret|pin|otp|key|code/i.test(name)

/** True when Astro will route this request to an action. */
const isActionPost = (request: Request, url: URL): boolean =>
    request.method === "POST" &&
    (url.searchParams.has("_action") || url.pathname.startsWith("/_actions/"))

/**
 * Reads the fields of a posted form, so a page can fill them in again after a
 * refusal. A player who mistypes one field keeps what they typed in the others.
 *
 * Only an action post is read, and only up to `MAX_BYTES`, so a stranger cannot
 * make the site buffer a body of any size. The request is cloned, so the action
 * still reads the body itself. A password is never kept: it would then sit in
 * the HTML of the answer.
 */
export const submittedFields = async (request: Request, url: URL): Promise<Submitted> => {
    if (!isActionPost(request, url)) return EMPTY

    const type = request.headers.get("content-type") ?? ""
    if (!FORM_TYPES.some(form => type.includes(form))) return EMPTY

    const declared = Number(request.headers.get("content-length"))
    // An absent or unreadable length is not a form of ours either: Astro sets
    // one for every action post the site makes.
    if (!Number.isFinite(declared) || declared <= 0 || declared > MAX_BYTES) {
        return EMPTY
    }

    return readFields(request).catch(() => EMPTY)
}

const readFields = async (request: Request): Promise<Submitted> => {
    const form = await request.clone().formData()
    return Object.fromEntries(
        [...form.entries()].flatMap(([name, value]) =>
            typeof value === "string" && !isSecret(name) ? [[name, value]] : [],
        ),
    )
}
