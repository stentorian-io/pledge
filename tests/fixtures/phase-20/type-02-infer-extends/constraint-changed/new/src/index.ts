export type FirstIfString<T> = T extends [infer S extends number, ...unknown[]] ? S : never;
