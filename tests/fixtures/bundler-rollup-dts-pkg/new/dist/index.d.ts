// rollup-plugin-dts output — new version merges Foo$1 into Foo
// After normalization, useFoo(f: Foo$1) maps to useFoo(f: Foo)
interface Foo {
  x: number;
  y: string;
}
declare function useFoo(f: Foo): void;
export { Foo, useFoo };
