// Requests share only the loaded compiler module, never a Program or source cache.
const readline = require('node:readline');
async function serve() {
  const requests = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of requests) {
    const { sequence, input } = JSON.parse(line);
    let output = JSON.stringify({ sequence, response: evaluate(input) });
    if (Buffer.byteLength(output) + 1 > 16 * 1024 * 1024) {
      output = JSON.stringify({ sequence, response: { results: [], issues: ['compiler output budget exhausted'] } });
    }
    process.stdout.write(output + '\n');
  }
}
serve().catch(error => { console.error(error); process.exit(1); });
