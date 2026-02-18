import { tool } from "ai";
import { z } from "zod";
import sql from "@/lib/db/ai-sdk";

interface MemoryRecord {
  id: string;
  content: string;
  category: string | null;
  tags: string[];
  created_at: string;
  updated_at: string;
}

interface MemoryToolResult {
  action: string;
  success: boolean;
  data?: MemoryRecord | MemoryRecord[];
  count?: number;
  error?: string;
}

export const memoryTool = tool({
  description:
    "Save, search, or delete long-term memories. Use this to remember facts " +
    "about the user, project context, or important information across conversations.",
  inputSchema: z.object({
    action: z.enum(["save", "search", "delete"]).describe(
      "save: store a new memory, search: find relevant memories, delete: remove by ID",
    ),
    content: z
      .string()
      .optional()
      .describe("Memory content to save, or search query text"),
    category: z
      .string()
      .optional()
      .describe("Category: preference, fact, context, project"),
    tags: z
      .array(z.string())
      .optional()
      .describe("Tags for organizing memories"),
    memoryId: z
      .string()
      .optional()
      .describe("Memory ID (UUID) for deletion"),
  }),
  execute: async ({ action, content, category, tags, memoryId }): Promise<MemoryToolResult> => {
    try {
      switch (action) {
        case "save": {
          if (!content?.trim()) {
            return { action, success: false, error: "content is required to save a memory" };
          }
          const [memory] = await sql`
            INSERT INTO memories (content, category, tags)
            VALUES (${content}, ${category ?? null}, ${tags ?? []})
            RETURNING id, content, category, tags, created_at, updated_at
          `;
          return { action, success: true, data: memory as MemoryRecord };
        }

        case "search": {
          if (!content?.trim()) {
            // No query — return recent memories
            const memories = await sql`
              SELECT id, content, category, tags, created_at, updated_at
              FROM memories
              ORDER BY updated_at DESC
              LIMIT 10
            `;
            return {
              action,
              success: true,
              count: memories.length,
              data: memories as unknown as MemoryRecord[],
            };
          }

          // Full-text search: join terms with & for AND matching
          const tsquery = content
            .split(/\s+/)
            .filter(Boolean)
            .join(" & ");

          const memories = await sql`
            SELECT
              id, content, category, tags, created_at, updated_at,
              ts_rank(to_tsvector('english', content), to_tsquery('english', ${tsquery})) AS rank
            FROM memories
            WHERE to_tsvector('english', content) @@ to_tsquery('english', ${tsquery})
            ORDER BY rank DESC, updated_at DESC
            LIMIT 10
          `;
          return {
            action,
            success: true,
            count: memories.length,
            data: memories as unknown as MemoryRecord[],
          };
        }

        case "delete": {
          if (!memoryId) {
            return { action, success: false, error: "memoryId is required for deletion" };
          }
          const [deleted] = await sql`
            DELETE FROM memories WHERE id = ${memoryId} RETURNING id
          `;
          if (!deleted) {
            return { action, success: false, error: `Memory ${memoryId} not found` };
          }
          return { action, success: true };
        }

        default:
          return { action, success: false, error: `Unknown action: ${action}` };
      }
    } catch (error) {
      return {
        action,
        success: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  },
});
