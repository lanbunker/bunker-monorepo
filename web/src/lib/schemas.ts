import { z } from "zod"

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

export const byId = z.object({ id: uuid })

export const handleInput = z.object({ handle })

export const playerHandleInput = z.object({ id: uuid, handle })

export const roleInput = z.object({
    id: uuid,
    role: z.enum(["user", "admin"], "Pick user or admin."),
})

/**
 * Where to go after a login or a signup. Only a path of this site: one slash,
 * then neither a second slash nor a backslash, and no backslash anywhere,
 * because a browser reads `/\evil.example` like `//evil.example`. Only printable
 * ASCII, because a control character in a `Location` header fails the response.
 * An empty field, the usual case, means the profile.
 */
export const localPath = z
    .string()
    .nullish()
    .transform(value => value || undefined)
    .pipe(
        z
            .string()
            .regex(/^\/(?![/\\])[\x21-\x5b\x5d-\x7e]*$/, "That link is not valid.")
            .optional(),
    )

export const credentials = z.object({ handle, password, next: localPath })

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

/**
 * A text the player can leave empty. An empty field arrives as null from a
 * form post, and the API wants a string, so null becomes "".
 */
const optionalText = (max: number, rule: string) =>
    z
        .string()
        .max(max, rule)
        .nullish()
        .transform(value => (value ?? "").trim())

const description = optionalText(1000, "A description is at most 1000 characters.")

const SKILL_RULE = "Pick a level from 1 to 5."

/** A radio group posts its value as text. The API reads a number from 1 to 5. */
export const skillLevel = z.coerce
    .number({ error: SKILL_RULE })
    .int(SKILL_RULE)
    .min(1, SKILL_RULE)
    .max(5, SKILL_RULE)

/** The empty option of a select means no level. */
export const optionalSkillLevel = z.preprocess(
    value => (value === "" || value === null ? undefined : value),
    skillLevel.optional(),
)

const AMOUNT_RULE = "An adjustment is a whole number from -10000 to 10000, not zero."

/** A signed number of cycles from the backoffice form. The API holds the same bound. */
export const adjustmentInput = z.object({
    id: uuid,
    handle,
    amount: z.coerce
        .number({ error: AMOUNT_RULE })
        .int(AMOUNT_RULE)
        .min(-10_000, AMOUNT_RULE)
        .max(10_000, AMOUNT_RULE)
        .refine(value => value !== 0, AMOUNT_RULE),
    note: z
        .string()
        .trim()
        .min(1, "Give a reason for the adjustment.")
        .max(200, "A note is at most 200 characters."),
})

const tournamentFields = {
    name: z
        .string()
        .trim()
        .min(1, "A tournament needs a name.")
        .max(60, "A tournament name is at most 60 characters."),
    game: z
        .string()
        .trim()
        .min(1, "A tournament needs a game.")
        .max(40, "A game is at most 40 characters."),
    mode: z
        .string()
        .trim()
        .min(1, "A tournament needs a mode.")
        .max(30, "A mode is at most 30 characters."),
    description,
    date: z.iso.date("Pick a date."),
    registrationClosesAt: z.iso.datetime({
        offset: true,
        error: "Pick the moment registration closes.",
    }),
}

export const tournamentInput = z.object(tournamentFields)

export const tournamentUpdateInput = z.object({ id: uuid, ...tournamentFields })

export const applyInput = z.object({ id: uuid, skill: skillLevel })

export const entrantInput = z.object({ id: uuid, handle, skill: optionalSkillLevel })

export const entrantRemovalInput = z.object({ id: uuid, entrantId: uuid })

export const seedOrderInput = z.object({
    id: uuid,
    entrants: z.array(uuid).min(2, "A bracket needs at least two entrants."),
})

export const resultInput = z.object({ id: uuid, matchId: uuid, winner: uuid })

export const matchInput = z.object({ id: uuid, matchId: uuid })

export const statusChangeInput = z.object({
    id: uuid,
    status: z.enum(["open", "live", "concluded"], "Pick a status."),
    // The select is absent on most forms and its empty option is "". Both mean
    // no winner.
    winner: z
        .string()
        .nullish()
        .transform(value => value || undefined)
        .pipe(uuid.optional()),
})

const eventFields = {
    name: z
        .string()
        .trim()
        .min(1, "An event needs a name.")
        .max(60, "An event name is at most 60 characters."),
    location: optionalText(60, "A location is at most 60 characters."),
    games: optionalText(200, "The games line is at most 200 characters."),
    description,
    /** The empty option of the select means no cover. */
    image: z
        .string()
        .nullish()
        .transform(value => value || undefined)
        .pipe(
            z
                .string()
                .regex(/^[A-Za-z0-9._-]{1,80}$/, "Pick a cover from the list.")
                .optional(),
        ),
    startsAt: z.iso.datetime({ offset: true, error: "Pick when the doors open." }),
    endsAt: z.iso.datetime({ offset: true, error: "Pick when the night ends." }),
}

const inOrder = (input: { startsAt: string; endsAt: string }) =>
    new Date(input.endsAt).getTime() > new Date(input.startsAt).getTime()

const WINDOW_RULE = { message: "The end must come after the start.", path: ["endsAt"] }

export const eventInput = z.object(eventFields).refine(inOrder, WINDOW_RULE)

export const eventUpdateInput = z
    .object({ id: uuid, ...eventFields })
    .refine(inOrder, WINDOW_RULE)

export const eventStatusInput = z.object({
    id: uuid,
    status: z.enum(["draft", "published"], "Pick a status."),
})

/** The secret in a check-in link. The API has the same shape. */
export const checkinCode = z.string().regex(/^[a-z0-9]{12}$/, "That link is not valid.")

export const checkinInput = z.object({ code: checkinCode })
