// WR-03 regression fixture (old):
// `bar` is DECORATED in the class body (`@log static bar = 'x'`).
// The static block ALSO writes `Foo.bar = 'y'` — a runtime assignment
// to the same slot. The decorated class-body declaration must win in
// synthesis so its decorators survive into the diff layer, allowing
// the decorator change to be detected end-to-end.
declare function log(...args: unknown[]): unknown;
declare function trace(...args: unknown[]): unknown;
export class Foo {
  @log static bar = 'x';
  static {
    Foo.bar = 'y';
  }
}
