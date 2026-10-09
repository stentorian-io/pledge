// BUN-11-A: UMD factory returns object literal — old has the symbol that new drops.
(function (root, factory) {
  if (typeof define === 'function' && define.amd) define([], factory);
  else if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.MyLib = factory();
}(typeof self !== 'undefined' ? self : this, function () {
  return {
    foo: function() { return 1; },
    bar: function() { return 2; },
    removed_symbol: 'will be dropped',
    VERSION: '1.0.0'
  };
}));
