// esbuild-generated CJS output — new version removes bar (breaking change)
"use strict";
var __defProp = Object.defineProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};

// 0 && (module.exports = {...}) tree-shake wrapper hint (BUN-01)
0 && (module.exports = {
  foo: null
});

var index_exports = {};
__export(index_exports, {
  foo: () => foo
});
module.exports = __toCommonJS(index_exports);

function foo() { return 1; }
