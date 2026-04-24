// CLS-12 dynamic-key fixture (new):
// Subscript-expression LHS on both sides — limitation annotation
// fires (either side's flag triggers the emission).
declare const key: string;
export class Registry {
  static {
    Registry[key] = 'y';
  }
}
