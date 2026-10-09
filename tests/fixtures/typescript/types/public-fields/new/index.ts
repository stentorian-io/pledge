// Renamed type alias — same shape (STRUCT-01/STRUCT-02: should NOT produce false positive)
export type Settings = {
  host: string;
  port: number;
};

// Nested field type changed: database.ssl removed (STRUCT-03: should detect)
export type AppSettings = {
  database: {
    host: string;
    port: number;
  };
  cache: {
    ttl: number;
  };
};

// Function now takes Settings instead of Config (same shape — should be fine if structural)
export function configure(opts: Settings): void {}

// Type changed from number to string (should detect type-changed)
export const timeout: string = "5000";

// Field removed: age (should detect field-removed)
export type User = {
  id: string;
  name: string;
  email: string;
};
