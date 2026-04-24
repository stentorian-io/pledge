// CLS-12 this-assignment-anon-class fixture (old):
// Anonymous default-exported class — `class_name` is empty so
// `Foo.bar =` cannot match; the `this.bar =` path must handle it.
export default class {
  static {
    this.version = '1.0.0';
  }
}
