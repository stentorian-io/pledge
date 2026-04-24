const api = {};
exports[Symbol.asyncIterator] = async function* () { yield 1; yield 2; };
module.exports = api;
