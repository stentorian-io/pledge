module.exports = new Proxy({}, { get: () => process.env.VALUE });
