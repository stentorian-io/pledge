// esbuild-generated CJS output — exercises BUN-01, BUN-02, BUN-03
"use strict";
var __defProp = Object.defineProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};

// 0 && (module.exports = {...}) tree-shake wrapper hint (BUN-01)
0 && (module.exports = {
  foo: null,
  bar: null
});

var index_exports = {};
__export(index_exports, {
  foo: () => foo,
  bar: () => bar
});
module.exports = __toCommonJS(index_exports);

function foo() { return 1; }
function bar() { return 2; }
