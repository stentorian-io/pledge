// WR-01 regression fixture (new):
// Private decorator factory changed @log -> @trace. Public surface
// (`bump()`) is unchanged. Expected: ZERO decorator findings — the
// change is purely on a private member and must not leak.
declare function log(...args: unknown[]): unknown;
declare function trace(...args: unknown[]): unknown;
export class Counter {
  @trace #count: number = 0;
  bump(): void {
    this.#count += 1;
  }
}
