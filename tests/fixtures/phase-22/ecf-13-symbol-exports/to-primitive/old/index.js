const value = {};
module.exports = value;
module.exports[Symbol.toPrimitive] = function (hint) { return 42; };
