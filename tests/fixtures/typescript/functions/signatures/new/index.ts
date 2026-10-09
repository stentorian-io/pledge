// SIG-01: added required param 'greeting' — should produce param-added Major
export function greet(name: string, greeting: string): string {
  return `${greeting} ${name}`;
}

// SIG-02: param type narrowed from string | number to string — should produce param-type-narrowed Major
export function process(input: string): void {
  console.log(input);
}

// SIG-03: return type widened from string to string | number — should produce return-type-widened Major
export function getData(): string | number {
  return "data";
}

// SIG-04: added optional param — should produce param-added Minor
export function configure(host: string, port?: number): void {
  console.log(host, port);
}

// SIG-05: param order changed from (a, b) to (b, a) — should produce param-reordered Major
export function transform(b: number, a: string): string {
  return a + b;
}

// DET-03: new named export — should produce export-added Minor
export function newFeature(): void {}

// Existing export unchanged
export const VERSION = "1.0.0";
