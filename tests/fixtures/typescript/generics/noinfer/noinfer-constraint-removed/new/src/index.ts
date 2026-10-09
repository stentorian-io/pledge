export type Base = { id: string };
export interface Store<T extends Base> {
  item: T;
}
