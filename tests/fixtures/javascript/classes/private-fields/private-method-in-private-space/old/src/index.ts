export class Calc {
  run(x: number): number { return this.#compute(x); }
  #compute(x: number): number { return x * 2; }
}
