import { glob } from "astro/loaders"
import { z } from "astro/zod"
import { defineCollection } from "astro:content"

const about = defineCollection({
    loader: glob({ pattern: "about.md", base: "./src/content" }),
    schema: z.object({
        title: z.string(),
    }),
})

export const collections = { about }
