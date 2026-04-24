class Range {
  constructor(start, end) { this.start = start; this.end = end; }
}
module.exports = Range;
module.exports[Symbol.iterator] = function* (self) {
  for (let i = self.start; i < self.end; i++) yield i;
};
