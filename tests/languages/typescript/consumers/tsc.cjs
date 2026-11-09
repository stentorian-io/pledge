// Run the selected CLI unchanged, with fresh module state for every consumer.
// Only Node startup and JavaScript compilation are shared; no Pledge code runs here.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createRequire } = require('node:module');

const library = process.argv[1];
// Recent TypeScript distributions put the CLI behind a compile-cache shim.
const cli = path.join(library, fs.existsSync(path.join(library, '_tsc.js')) ? '_tsc.js' : 'tsc.js');
const invoke = vm.compileFunction(fs.readFileSync(cli, 'utf8'),
  ['require', 'module', 'exports', '__filename', '__dirname', 'process'], { filename: cli });
const requests = JSON.parse(fs.readFileSync(0, 'utf8'));
const outcomes = [];
for (const { id, root, args } of requests) {
  process.chdir(root);
  const stopped = Symbol('compiler exit');
  let code, stdout = '', stderr = '';
  const compilerProcess = new Proxy(process, { get(target, key) {
    if (key === 'argv') return [process.execPath, cli, ...args];
    if (key === 'exit') return status => { code = status; throw stopped; };
    if (key === 'stdout') return { isTTY: false, write: chunk => { stdout += chunk; return true; } };
    if (key === 'stderr') return { isTTY: false, write: chunk => { stderr += chunk; return true; } };
    return Reflect.get(target, key);
  } });
  const moduleRecord = { exports: {} };
  try {
    invoke(createRequire(cli), moduleRecord, moduleRecord.exports, cli, library, compilerProcess);
  } catch (error) {
    if (error !== stopped) throw error;
  }
  if (!Number.isInteger(code)) throw new Error(`consumer ${id}: CLI did not finish synchronously`);
  outcomes.push({ id, code, stdout, stderr });
}
process.stdout.write(JSON.stringify(outcomes));
