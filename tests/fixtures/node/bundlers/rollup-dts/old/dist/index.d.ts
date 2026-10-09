// rollup-plugin-dts output — exercises SEM-08 ($1 suffix normalization)
// Foo$1 is a rollup disambiguation suffix when multiple files define Foo
interface Foo {
  x: number;
}
interface Foo$1 {
  y: string;
}
declare function useFoo(f: Foo$1): void;
export { Foo, useFoo };
