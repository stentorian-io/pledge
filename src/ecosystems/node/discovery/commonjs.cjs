'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { createRequire, isBuiltin } = require('node:module');
const { fileURLToPath, pathToFileURL } = require('node:url');

const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const root = fs.realpathSync(input.root);
function readManifest(filename) {
  if (fs.statSync(filename).size > input.maxSourceBytes) {
    throw new Error('package metadata exceeds the CommonJS analysis limit');
  }
  return JSON.parse(fs.readFileSync(filename, 'utf8'));
}
const manifest = readManifest(path.join(root, 'package.json'));
const nativeSources = process.binding('natives');
let parse;

// Evaluate only the trusted parser bundled into this Node executable. Package
// source is passed to parse as a string, never evaluated or required.
const legacyLexer = nativeSources['internal/deps/cjs-module-lexer/lexer'];
if (legacyLexer) {
  const parserModule = { exports: {} };
  new Function('module', 'exports', legacyLexer)(parserModule, parserModule.exports);
  parse = source => parserModule.exports.parse(source);
} else {
  const { internalBinding } = require('internal/test/binding');
  const lexer = internalBinding('cjs_lexer');
  parse = source => {
    const [exports, reexports] = lexer.parse(source);
    return { exports: [...exports], reexports };
  };
}
const { defaultResolve } = require('internal/modules/esm/resolve');
const { getDefaultConditions } = require('internal/modules/esm/utils');
const conditions = getDefaultConditions();
const parentURL = pathToFileURL(path.join(root, '__pledge_import__.mjs')).href;
const requireFromRoot = createRequire(path.join(root, '__pledge_require__.cjs'));
const unconditionalModuleMarker = Number(process.versions.node.split('.')[0]) >= 23;
const modules = new Map();
let sourceBytes = 0;

function contained(filename) {
  const canonical = fs.realpathSync(filename);
  const relative = path.relative(root, canonical);
  if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
    throw new Error('CommonJS re-export resolves outside the prepared package');
  }
  return canonical;
}

function sourceType(filename) {
  const extension = path.extname(filename);
  if (extension === '.cjs') return 'commonjs';
  if (['.mjs', '.json', '.node'].includes(extension)) return 'other';
  if (['.ts', '.cts', '.mts'].includes(extension)) {
    throw new Error('TypeScript runtime re-export syntax is not supported by the CommonJS name check');
  }
  if (extension !== '.js') return 'commonjs';
  let directory = path.dirname(filename);
  for (let depth = 0; depth < input.maxDepth; depth++) {
    const packageFile = path.join(directory, 'package.json');
    if (fs.existsSync(packageFile)) {
      const type = readManifest(packageFile).type;
      return type === 'module' ? 'other' : type === 'commonjs' ? 'commonjs' : 'ambiguous';
    }
    if (directory === root) return 'ambiguous';
    directory = path.dirname(directory);
  }
  throw new Error('package scope depth limit reached');
}

function readModule(filename) {
  filename = contained(filename);
  if (modules.has(filename)) {
    const cached = modules.get(filename);
    if (cached.error) throw new Error(cached.error);
    return cached;
  }
  if (modules.size >= input.maxModules) throw new Error('CommonJS module count limit reached');
  try {
    const type = sourceType(filename);
    if (type === 'other') {
      const result = { other: true, exports: [], dependencies: [] };
      modules.set(filename, result);
      return result;
    }
    const size = fs.statSync(filename).size;
    if (size > input.maxSourceBytes || sourceBytes + size > input.maxTotalBytes) {
      throw new Error('CommonJS source byte limit reached');
    }
    const source = fs.readFileSync(filename, 'utf8');
    sourceBytes += size;
    if (source.split('\n').some(line => Buffer.byteLength(line) > input.maxLineBytes)) {
      throw new Error('CommonJS source line limit reached');
    }
    if (type === 'ambiguous') {
      const { containsModuleSyntax } = process.binding('contextify');
      if (typeof containsModuleSyntax !== 'function') {
        throw new Error('the installed Node syntax detector is unavailable');
      }
      if (containsModuleSyntax(source, filename)) {
        const result = { other: true, exports: [], dependencies: [] };
        modules.set(filename, result);
        return result;
      }
    }
    let parsed;
    try {
      parsed = parse(source);
    } catch (error) {
      if (type === 'ambiguous' && error.code === 'ERR_LEXER_ESM_SYNTAX') {
        const result = { other: true, exports: [], dependencies: [] };
        modules.set(filename, result);
        return result;
      }
      throw new Error('the installed Node parser could not analyze CommonJS names');
    }
    if (!Array.isArray(parsed.exports) || !Array.isArray(parsed.reexports)) {
      throw new Error('the installed Node parser returned an unsupported result');
    }
    if (parsed.exports.length > input.maxNames || parsed.reexports.length > input.maxModules) {
      throw new Error('CommonJS export count limit reached');
    }
    const requireFromModule = createRequire(filename);
    const dependencies = [];
    for (const specifier of parsed.reexports) {
      if (isBuiltin(specifier)) continue;
      dependencies.push(contained(requireFromModule.resolve(specifier)));
    }
    const result = { exports: parsed.exports, dependencies };
    modules.set(filename, result);
    return result;
  } catch (error) {
    modules.set(filename, { error: error.message });
    throw error;
  }
}

function collectNames(filename, names, visited, active, depth) {
  if (depth >= input.maxDepth) throw new Error('CommonJS re-export depth limit reached');
  filename = contained(filename);
  if (active.has(filename)) throw new Error('cyclic CommonJS re-exports leave import order uncertain');
  if (visited.has(filename)) return;
  active.add(filename);
  const module = readModule(filename);
  for (const name of module.exports) {
    if (name !== 'default' && !(unconditionalModuleMarker && name === 'module.exports')) {
      names.add(name);
    }
  }
  if (names.size > input.maxNames) throw new Error('CommonJS export count limit reached');
  for (const dependency of module.dependencies) {
    collectNames(dependency, names, visited, active, depth + 1);
  }
  active.delete(filename);
  visited.add(filename);
}

function selected(entry) {
  let target;
  if (manifest.exports !== undefined && manifest.exports !== null) {
    if (typeof manifest.name !== 'string') {
      throw new Error('package name is required to verify conditional named imports');
    }
    const specifier = manifest.name + (entry.subpath === '.' ? '' : entry.subpath.slice(1));
    try {
      target = fileURLToPath(defaultResolve(specifier, { parentURL, conditions }).url);
    } catch (error) {
      if (error.code === 'ERR_PACKAGE_PATH_NOT_EXPORTED') return false;
      throw error;
    }
  } else {
    target = entry.subpath === '.' ? requireFromRoot.resolve(root) : path.join(root, entry.subpath);
  }
  return contained(target) === entry.path;
}

const results = input.entries.map(entry => {
  const result = { ...entry, names: null, error: null };
  try {
    if (!selected(entry) || readModule(entry.path).other) return result;
    const names = new Set();
    collectNames(entry.path, names, new Set(), new Set(), 0);
    result.names = [...names].sort();
  } catch (error) {
    result.error = error.message;
  }
  return result;
});
process.stdout.write(JSON.stringify({ node: process.version, entries: results }));
