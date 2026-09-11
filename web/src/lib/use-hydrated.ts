import { useSyncExternalStore } from "react"

const NEVER_CHANGES = () => () => {}

/**
 * False while the server renders and during the first client render, true
 * afterwards. An island disables its controls until this is true, so a click on
 * the server-rendered markup cannot get lost.
 */
export const useHydrated = (): boolean =>
    useSyncExternalStore(
        NEVER_CHANGES,
        () => true,
        () => false,
    )
