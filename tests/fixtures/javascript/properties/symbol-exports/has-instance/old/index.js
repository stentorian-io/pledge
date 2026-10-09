class Marker {}
module.exports = Marker;
module.exports[Symbol.hasInstance] = function (obj) { return obj && obj.isMarker === true; };
