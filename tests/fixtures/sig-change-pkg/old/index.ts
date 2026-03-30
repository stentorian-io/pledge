// SIG-01 target: greet has 1 required param
export function greet(name: string): string {
  return `Hello ${name}`;
}

// SIG-02 target: process takes string | number param
export function process(input: string | number): void {
  console.log(input);
}

// SIG-03 target: getData returns string
export function getData(): string {
  return "data";
}

// SIG-04 target: configure has 1 param (new optional param will be added)
export function configure(host: string): void {
  console.log(host);
}

// SIG-05 target: transform has (a, b) param order
export function transform(a: string, b: number): string {
  return a + b;
}

// DET-03/DET-04 target: this export exists in old (newFeature will only be in new)
export const VERSION = "1.0.0";
