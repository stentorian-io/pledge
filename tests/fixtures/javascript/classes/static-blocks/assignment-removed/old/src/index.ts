// CLS-12 assignment-removed fixture (old):
// `Registry.defaults` is assigned in a static initialization block.
// The new version drops it entirely — consumers reading
// `Registry.defaults` will break.
export class Registry {
  static {
    Registry.defaults = { theme: 'light', locale: 'en' };
  }
}
