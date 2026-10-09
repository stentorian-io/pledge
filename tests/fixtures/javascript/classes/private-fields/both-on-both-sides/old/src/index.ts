export class Brand {
  foo: number = 0;
  #foo: number = 0;
  get(): number { return this.foo + this.#foo; }
}
