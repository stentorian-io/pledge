class Range {
  constructor(start, end) { this.start = start; this.end = end; }
  [Symbol.iterator]() {
    let i = this.start;
    const end = this.end;
    return { next() { return i < end ? { value: i++, done: false } : { value: undefined, done: true }; } };
  }
}
module.exports = Range;
