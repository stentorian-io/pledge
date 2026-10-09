function Log(_t: any, _k: any, _d: any) {}
export class Foo {
  @Log
  bar(): number {
    return 1;
  }
}
