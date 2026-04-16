import { glob } from "astro/loaders"
import { z } from "astro/zod"
import { defineCollection } from "astro:content"

const events = defineCollection({
    loader: glob({ pattern: "**/*.md", base: "./src/content/events" }),
    schema: z.object({
        date: z.string(),
        name: z.string(),
        status: z.string(),
        slots: z.string(),
        games: z.string(),
        image: z.string(),
    }),
})

const about = defineCollection({
    loader: glob({ pattern: "about.md", base: "./src/content" }),
    schema: z.object({
        title: z.string(),
    }),
})

export const collections = { events, about }
