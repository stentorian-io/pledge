// CLS-12 assignment-removed fixture (new):
// `Registry.defaults` dropped — Major `class-method-removed`
// with `static property` noun should fire.
export class Registry {
  static {
    // defaults dropped — consumers reading Registry.defaults will break
  }
}
