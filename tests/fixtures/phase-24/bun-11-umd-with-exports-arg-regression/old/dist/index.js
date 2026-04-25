// BUN-11 regression: UMD with exports arg — already detected via depth-agnostic
// CJS_EXPORTS_PROPERTY_QUERY. Must remain green after Phase 24.
(function (root, factory) {
  if (typeof define === 'function' && define.amd) define(['exports'], factory);
  else if (typeof exports !== 'undefined') factory(exports);
  else factory((root.MyLib = {}));
})(this, function (exports) {
  exports.foo = function() { return 1; };
  exports.bar = 42;
  Object.defineProperty(exports, "qux", { value: 1, enumerable: true });
  exports.removed_symbol = 'gone in new';
});
