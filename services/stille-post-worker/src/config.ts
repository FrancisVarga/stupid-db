export interface Config {
  databaseUrl: string;
  anthropicApiKey: string;
  apiBase: string;
  port: number;
}

export function loadConfig(): Config {
  const databaseUrl = process.env.DATABASE_URL;
  if (!databaseUrl) {
    throw new Error("DATABASE_URL environment variable is required");
  }

  const anthropicApiKey = process.env.ANTHROPIC_API_KEY;
  if (!anthropicApiKey) {
    throw new Error("ANTHROPIC_API_KEY environment variable is required");
  }

  return {
    databaseUrl,
    anthropicApiKey,
    apiBase: process.env.API_BASE ?? "http://localhost:3000",
    port: parseInt(process.env.PORT ?? "4100", 10),
  };
}
