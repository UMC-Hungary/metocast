import { z } from 'zod';

export const BiblePassageSchema = z.object({
  label: z.string(),
  verses: z.array(
    z.object({
      chapter: z.number(),
      verse: z.number(),
      text: z.string(),
    }),
  ),
});

export const BibleSuggestionSchema = z.object({
  cat: z.string(),
  label: z.string(),
  link: z.string(),
});

export const BibleSuggestionsSchema = z.array(BibleSuggestionSchema);

export type BiblePassage = z.infer<typeof BiblePassageSchema>;
export type BibleSuggestion = z.infer<typeof BibleSuggestionSchema>;
