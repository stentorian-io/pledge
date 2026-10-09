export class Calc {
  run(x: number): number { return this.#recalculate(x); }
  #recalculate(x: number): number { return x * 2; }
}
