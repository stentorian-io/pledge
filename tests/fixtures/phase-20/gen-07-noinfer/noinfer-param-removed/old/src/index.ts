export function createStore<T>(
  initial: T,
  update: (prev: NoInfer<T>) => NoInfer<T>
): T;
export function createStore<T>(
  initial: T,
  update: (prev: NoInfer<T>) => NoInfer<T>
): T {
  return initial;
}
