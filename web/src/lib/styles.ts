/**
 * Class strings that two or more files share. A component owns its own classes;
 * this file holds only what would otherwise be copied.
 */

/** A text field in the backoffice. */
export const ADMIN_FIELD =
    "w-full border-b border-warn/40 bg-transparent px-1 py-1 text-xs text-admin-ink outline-none focus:border-warn"

/** A narrow inline field in the backoffice, such as a handle box in a row. */
export const ADMIN_FIELD_INLINE =
    "border-b border-warn/40 bg-transparent px-1 py-0.5 text-xs text-admin-ink outline-none focus:border-warn"

/** The list of a log panel: the cycles log and the match log read as one. */
export const LOG_LIST = "m-0 flex list-none flex-col p-0 text-xs"

/** One line of a log panel. The first column is the amount or the result. */
export const LOG_ROW =
    "grid grid-cols-[4.5rem_minmax(0,1fr)_auto] items-baseline gap-x-3 border-t border-faint py-2 first:border-t-0 hover:bg-faint/40"

/** A text field on the public site. */
export const SITE_FIELD =
    "w-full border-b border-border bg-transparent py-1 text-ink outline-none focus:border-accent"
