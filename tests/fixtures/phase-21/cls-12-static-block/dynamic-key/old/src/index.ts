// CLS-12 dynamic-key fixture (old):
// Subscript-expression LHS — Interlint declines to extract the
// member and emits the `(limitation: dynamic-static-member-key)`
// annotation.
declare const key: string;
export class Registry {
  static {
    Registry[key] = 'x';
  }
}
