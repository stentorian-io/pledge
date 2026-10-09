// BUN-10 — Object.assign({}, exports, {…}) variant; old has both `removed` and `kept`
"use strict";
exports.foo = function() { return 1; };
module.exports = Object.assign({}, exports, { removed: 2, kept: 3 });
