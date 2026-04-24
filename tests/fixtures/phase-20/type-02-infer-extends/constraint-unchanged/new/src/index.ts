export type FirstIfString<T> = T extends [infer S extends string, ...unknown[]] ? S : never;
export type Helper = string;
