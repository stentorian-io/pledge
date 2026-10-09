// WR-01 regression fixture (old):
// Private field `#count` carries `@log` factory. Public method `bump()`
// is undecorated. The decorator change is on the PRIVATE member only.
declare function log(...args: unknown[]): unknown;
declare function trace(...args: unknown[]): unknown;
export class Counter {
  @log #count: number = 0;
  bump(): void {
    this.#count += 1;
  }
}
