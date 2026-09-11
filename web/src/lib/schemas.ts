import { z } from "zod"

/**
 * The shape of every form the site posts. The API validates again, and it is
 * the authority. These rules exist so that a typing mistake gets a sentence the
 * player can read instead of a round trip.
 */

const HANDLE_RULE = "A handle is 3 to 20 characters: letters, digits, _ . or -"
const PASSWORD_RULE = "A password is 8 to 128 characters."

// A hand-typed or stale identifier never reaches the API: the site refuses it
// first, and says so in terms the visitor can act on.
export const uuid = z.uuid("That link is not valid.")

export const handle = z
    .string()
    .trim()
    .min(3, HANDLE_RULE)
    .max(20, HANDLE_RULE)
    .regex(/^[A-Za-z0-9_.-]+$/, HANDLE_RULE)

export const password = z.string().min(8, PASSWORD_RULE).max(128, PASSWORD_RULE)

export const credentials = z.object({ handle, password })

export const signupInput = credentials
    .extend({ confirmPassword: password })
    .refine(input => input.password === input.confirmPassword, {
        message: "The two passwords differ.",
        path: ["confirmPassword"],
    })

export const passwordChangeInput = z
    .object({
        currentPassword: password,
        newPassword: password,
        confirmPassword: password,
    })
    .refine(input => input.newPassword === input.confirmPassword, {
        message: "The two new passwords differ.",
        path: ["confirmPassword"],
    })

/** An empty textarea arrives as null from a form post. */
const description = z
    .string()
    .max(1000, "A description is at most 1000 characters.")
    .nullish()
    .transform(value => (value ?? "").trim())

export const tournamentFields = {
    name: z.string().trim().min(1, "A tournament needs a name.").max(60),
    game: z.string().trim().min(1, "A tournament needs a game.").max(40),
    mode: z.string().trim().min(1, "A tournament needs a mode.").max(30),
    description,
    date: z.iso.date("Pick a date."),
    registrationClosesAt: z.iso.datetime({
        offset: true,
        error: "Pick the moment registration closes.",
    }),
}

export const tournamentInput = z.object(tournamentFields)

export const statusChangeInput = z.object({
    id: uuid,
    status: z.enum(["open", "live", "concluded"]),
    // The select is absent on most forms and its empty option is "". Both mean
    // no winner.
    winner: z
        .string()
        .nullish()
        .transform(value => value || undefined)
        .pipe(uuid.optional()),
})
