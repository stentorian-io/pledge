// CLS-12 assignment-added fixture (new):
// Added Registry.version — a new static property. Added static
// surface is not a breaking change; zero CLS-03 findings expected.
export class Registry {
  static {
    Registry.version = '1.0.0';
  }
}
