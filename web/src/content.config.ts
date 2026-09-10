import { glob } from "astro/loaders"
import { z } from "astro/zod"
import { defineCollection } from "astro:content"

const events = defineCollection({
    loader: glob({ pattern: "**/*.md", base: "./src/content/events" }),
    schema: ({ image }) =>
        z.object({
            date: z
                .string()
                .regex(/^(\d{2}\/\d{2}\/\d{4}|TBA)$/, "Use DD/MM/YYYY or TBA"),
            name: z.string(),
            status: z.enum(["OPEN", "CLOSED", "SOON"]),
            games: z.string(),
            image: image().optional(),
        }),
})

const about = defineCollection({
    loader: glob({ pattern: "about.md", base: "./src/content" }),
    schema: z.object({
        title: z.string(),
    }),
})

export const collections = { events, about }
