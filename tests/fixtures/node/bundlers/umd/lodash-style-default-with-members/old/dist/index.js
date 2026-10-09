// BUN-11-B: lodash-style IIFE with `module.exports = <ident>` — old has
// 4 compound members; new drops the one that is removed across versions.
(function() {
  var root = (typeof self == 'object' && self.self === self && self) ||
             (typeof global == 'object' && global.global === global && global) || this;
  var _ = function() {};
  _.chunk = function() { return 1; };
  _.compact = function() { return 2; };
  _.removed_member = function() { return 3; };
  _.VERSION = '1.0.0';
  if (typeof define == 'function' && typeof define.amd == 'object' && define.amd) {
    root._ = _;
    define(function() { return _; });
  } else if (typeof module == 'object' && module && module.exports) {
    (module.exports = _)._ = _;
  } else {
    root._ = _;
  }
}.call(this));
