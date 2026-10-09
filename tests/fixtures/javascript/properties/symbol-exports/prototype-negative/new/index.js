class Range {
  constructor(start, end) { this.start = start; this.end = end; }
}
Range.prototype[Symbol.iterator] = function* () {
  for (let i = this.start; i < this.end; i++) yield i;
};
module.exports = Range;
