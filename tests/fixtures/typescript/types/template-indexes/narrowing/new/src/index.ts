export interface EventMap {
  [key: `onClick${string}`]: (e: Event) => void;
}
