"use strict";
exports.foo = function() { return 1; };
module.exports = Object.assign({}, exports, { kept: 3 });
