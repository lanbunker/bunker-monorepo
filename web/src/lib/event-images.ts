import type { ImageMetadata } from "astro"

/**
 * The covers an event can carry. The files live in the repo and the API holds
 * only a file name, so a new cover is a commit and a deploy. A name the API
 * sends that is not here renders no cover, never an error.
 */
const COVERS = import.meta.glob<{ default: ImageMetadata }>(
    "/src/assets/images/events/*.webp",
    { eager: true },
)

const nameOf = (path: string): string => path.split("/").at(-1) ?? path

const BY_NAME: ReadonlyMap<string, ImageMetadata> = new Map(
    Object.entries(COVERS).map(([path, mod]) => [nameOf(path), mod.default]),
)

/** Every cover name, for the select in the backoffice. */
export const COVER_NAMES: readonly string[] = [...BY_NAME.keys()].toSorted()

export const coverOf = (name: string | null | undefined): ImageMetadata | undefined =>
    name ? BY_NAME.get(name) : undefined
