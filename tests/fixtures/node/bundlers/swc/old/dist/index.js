// swc-generated CJS output — exercises BUN-05 (Object.keys star re-export shim)
"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var _utils = require("./utils");
Object.keys(_utils).forEach(function(key) {
  if (key === "default" || key === "__esModule") return;
  if (key in exports && exports[key] === _utils[key]) return;
  Object.defineProperty(exports, key, {
    enumerable: true,
    get: function() { return _utils[key]; }
  });
});
