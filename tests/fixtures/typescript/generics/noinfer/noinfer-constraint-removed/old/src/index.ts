export type Base = { id: string };
export interface Store<T extends NoInfer<Base>> {
  item: T;
}
