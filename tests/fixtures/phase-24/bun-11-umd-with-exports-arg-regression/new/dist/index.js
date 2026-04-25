// BUN-11 regression: UMD with exports arg — new drops the symbol present in old.
(function (root, factory) {
  if (typeof define === 'function' && define.amd) define(['exports'], factory);
  else if (typeof exports !== 'undefined') factory(exports);
  else factory((root.MyLib = {}));
})(this, function (exports) {
  exports.foo = function() { return 1; };
  exports.bar = 42;
  Object.defineProperty(exports, "qux", { value: 1, enumerable: true });
});
