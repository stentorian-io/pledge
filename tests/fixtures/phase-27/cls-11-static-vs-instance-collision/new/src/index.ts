// WR-02 regression fixture (new):
// `static foo` removed; an INSTANCE private `#foo` added. The two slots
// differ in is_static — they are NOT the same logical slot. CLS-11
// must NOT emit class-field-made-private. (CLS-03 may emit
// class-method-removed for the removed static property; that is
// unrelated to WR-02 and the assertion below scopes only the
// class-field-made-private count.)
export class Container {
  #foo: number = 1;
}
