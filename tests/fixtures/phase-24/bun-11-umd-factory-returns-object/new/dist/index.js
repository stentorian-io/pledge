// BUN-11-A: UMD factory returns object literal — new drops the symbol present in old.
(function (root, factory) {
  if (typeof define === 'function' && define.amd) define([], factory);
  else if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.MyLib = factory();
}(typeof self !== 'undefined' ? self : this, function () {
  return {
    foo: function() { return 1; },
    bar: function() { return 2; },
    VERSION: '1.0.0'
  };
}));
