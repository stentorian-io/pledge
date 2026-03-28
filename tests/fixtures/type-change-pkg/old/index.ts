// Type alias (will be renamed in new version — STRUCT-01/STRUCT-02 test)
export type Config = {
  host: string;
  port: number;
};

// Nested object type (STRUCT-03 test)
export type AppSettings = {
  database: {
    host: string;
    port: number;
    ssl: boolean;
  };
  cache: {
    ttl: number;
  };
};

// Function with typed params (for function comparison)
export function configure(opts: Config): void {}

// Simple export that will have its type changed
export const timeout: number = 5000;

// Type that will have a field removed
export type User = {
  id: string;
  name: string;
  email: string;
  age: number;
};
