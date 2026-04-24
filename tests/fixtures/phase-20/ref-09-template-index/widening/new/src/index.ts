export interface EventMap {
  [key: `on${string}`]: (e: Event) => void;
}
