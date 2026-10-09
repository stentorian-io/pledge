export function createStore<T>(
  initial: T,
  update: (prev: NoInfer<T>) => T
): T;
export function createStore<T>(
  initial: T,
  update: (prev: NoInfer<T>) => T
): T {
  return initial;
}
