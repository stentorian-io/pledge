// WR-03 regression fixture (new):
// `bar`'s decorator changed @log -> @trace. CLS-10 decorator diff MUST
// fire because the decorated class-body declaration's decorators were
// preserved through the static_block synthesis pass on BOTH sides.
// Without class_body_wins, the synthesized member (no decorators)
// overwrites the class-body entry, losing the decorator entirely,
// and no decorator finding fires.
declare function log(...args: unknown[]): unknown;
declare function trace(...args: unknown[]): unknown;
export class Foo {
  @trace static bar = 'y';
  static {
    Foo.bar = 2;
  }
}
