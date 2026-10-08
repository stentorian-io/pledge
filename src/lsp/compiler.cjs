// Resolve public contracts with the same compiler installation as the language server.
// No package code is executed. Keep the wire format aligned with TypeShape's serde format.
const fs = require('node:fs');
const path = require('node:path');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const ts = require(input.compiler);
const issues = new Set();
let options = {
  target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.NodeNext,
  moduleResolution: ts.ModuleResolutionKind.NodeNext,
};
const configPath = ts.findConfigFile(input.root, ts.sys.fileExists);
if (configPath) {
  const config = ts.readConfigFile(configPath, ts.sys.readFile);
  if (config.error) issues.add(ts.flattenDiagnosticMessageText(config.error.messageText, ' '));
  else {
    const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configPath));
    options = { ...options, ...parsed.options };
    for (const error of parsed.errors) {
      // Root names come from discovery, so a package's excluded source tree is immaterial.
      if (error.code !== 18003) issues.add(ts.flattenDiagnosticMessageText(error.messageText, ' '));
    }
  }
}
// A public contract must retain nullability even when its author builds without strict mode.
Object.assign(options, { allowJs: true, checkJs: true, noEmit: true, skipLibCheck: true,
  strictNullChecks: true, strictFunctionTypes: true });
const program = ts.createProgram([...new Set(input.symbols.map(symbol => symbol.file))], options);
const checker = program.getTypeChecker();
const printer = ts.createPrinter();
const formatFlags = ts.NodeBuilderFlags.NoTruncation | ts.NodeBuilderFlags.InTypeAlias;
const primitive = name => ({ Primitive: name });
const generic = (name, args) => ({ Generic: { name, args } });
const builtins = new Set(['Promise', 'PromiseLike', 'Map', 'ReadonlyMap', 'Set', 'ReadonlySet',
  'WeakMap', 'WeakSet', 'Iterable', 'IterableIterator', 'Iterator', 'AsyncIterable',
  'AsyncIterableIterator', 'AsyncIterator', 'Generator', 'AsyncGenerator', 'Date', 'RegExp',
  'ArrayBuffer', 'SharedArrayBuffer', 'DataView', 'Error', 'Uint8Array', 'Int8Array',
  'Uint16Array', 'Int16Array', 'Uint32Array', 'Int32Array', 'Float32Array', 'Float64Array',
  'BigInt64Array', 'BigUint64Array']);
let packageWork = 200000;
let symbolWork;
let active;
let currentIssues;
let parameterNames;
let parameterScope;
function bindParameters(parameters) {
  const unbound = (parameters || []).filter(parameter => !parameterNames.has(parameter));
  if (!unbound.length) return;
  const scope = parameterScope++;
  unbound.forEach((parameter, index) => parameterNames.set(parameter, `T${scope}_${index}`));
}
function incomplete(message, type) {
  currentIssues.add(message);
  return { Opaque: type ? checker.typeToString(type) : '<unresolved>' };
}
function signatureShape(signature, depth) {
  bindParameters(signature.typeParameters);
  return { Function: {
    params: signature.getParameters().map(parameter => {
      const declaration = parameter.valueDeclaration || parameter.declarations?.[0];
      return { name: parameter.getName(),
        optional: !!(parameter.flags & ts.SymbolFlags.Optional) || !!declaration?.questionToken || !!declaration?.initializer,
        rest: !!declaration?.dotDotDotToken,
        shape: shape(checker.getTypeOfSymbol(parameter), depth + 1) };
    }),
    return_type: shape(checker.getReturnTypeOfSignature(signature), depth + 1),
  } };
}
function propertyName(symbol) {
  const declaration = symbol.valueDeclaration || symbol.declarations?.[0];
  return declaration?.name && ts.isComputedPropertyName(declaration.name)
    ? declaration.name.getText() : symbol.getName();
}
function isPrivate(symbol) {
  return symbol.declarations?.some(declaration =>
    declaration.name && ts.isPrivateIdentifier(declaration.name) ||
    declaration.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.PrivateKeyword || modifier.kind === ts.SyntaxKind.ProtectedKeyword));
}
function shape(type, depth = 0) {
  if (--symbolWork < 0 || --packageWork < 0 || depth > 32) return incomplete('compiler public type expansion limit reached', type);
  const flags = type.flags;
  for (const [flag, name] of [[ts.TypeFlags.Any, 'Any'], [ts.TypeFlags.Unknown, 'Unknown'],
    [ts.TypeFlags.Never, 'Never'], [ts.TypeFlags.String, 'String'], [ts.TypeFlags.Number, 'Number'],
    [ts.TypeFlags.Boolean, 'Boolean'], [ts.TypeFlags.Void, 'Void'], [ts.TypeFlags.Undefined, 'Undefined'],
    [ts.TypeFlags.Null, 'Null'], [ts.TypeFlags.ESSymbol, 'Symbol'], [ts.TypeFlags.BigInt, 'Bigint']]) {
    if (flags & flag) {
      if (type.intrinsicName === 'error') return incomplete('unresolved compiler type', type);
      return primitive(name);
    }
  }
  if (flags & ts.TypeFlags.StringLiteral) return { Literal: { String: type.value } };
  if (flags & ts.TypeFlags.NumberLiteral) return { Literal: { Number: String(type.value) } };
  if (flags & ts.TypeFlags.BigIntLiteral) return { Literal: { Number: `${type.value.negative ? '-' : ''}${type.value.base10Value}n` } };
  if (flags & ts.TypeFlags.Substitution) {
    const base = shape(type.baseType, depth + 1);
    return type.constraint.flags & ts.TypeFlags.Unknown ? { NoInferWrap: base }
      : { Intersection: [base, shape(type.constraint, depth + 1)] };
  }
  if (flags & ts.TypeFlags.BooleanLiteral) return { Literal: { Boolean: type.intrinsicName === 'true' } };
  if (flags & ts.TypeFlags.TypeParameter) {
    bindParameters([type]);
    return { TypeParam: parameterNames.get(type) };
  }
  if (active.has(type)) return { TypeParam: '<recursive>' };
  active.add(type);
  try {
    if (type.isUnion()) {
      // The compiler represents boolean as true | false. Preserve the parser's canonical form.
      if (type.types.length === 2 && type.types.every(member => member.flags & ts.TypeFlags.BooleanLiteral)) return primitive('Boolean');
      return { Union: type.types.map(member => shape(member, depth + 1)) };
    }
    if (type.isIntersection()) return { Intersection: type.types.map(member => shape(member, depth + 1)) };
    if (checker.isTupleType(type)) {
      const target = type.target;
      return { Tuple: checker.getTypeArguments(type).map((element, index) => {
        const elementFlags = target.elementFlags[index];
        const rest = !!(elementFlags & (ts.ElementFlags.Rest | ts.ElementFlags.Variadic));
        let elementShape = shape(element, depth + 1);
        if (elementFlags & ts.ElementFlags.Rest) elementShape = { Array: elementShape };
        return { label: target.labeledElementDeclarations?.[index]?.name?.getText() || null,
          optional: !!(elementFlags & ts.ElementFlags.Optional), rest, shape: elementShape };
      }) };
    }
    if (checker.isArrayType(type)) return { Array: shape(checker.getTypeArguments(type)[0], depth + 1) };
    const name = type.symbol?.getName();
    const isLibraryType = type.symbol?.declarations?.some(declaration => program.isSourceFileDefaultLibrary(declaration.getSourceFile()));
    if (isLibraryType && (builtins.has(name) || name === 'ReadonlyArray')) {
      const args = checker.getTypeArguments(type).map(argument => shape(argument, depth + 1));
      return generic(name, args);
    }
    if (flags & ts.TypeFlags.TemplateLiteral) {
      const parts = [];
      type.texts.forEach((text, index) => {
        parts.push({ Literal: text });
        if (index < type.types.length) parts.push({ Placeholder: shape(type.types[index], depth + 1) });
      });
      return { Template: parts };
    }
    if (flags & ts.TypeFlags.StringMapping) return generic(type.symbol.getName(), [shape(type.type, depth + 1)]);
    if (flags & ts.TypeFlags.Conditional) {
      // Building the node asks the compiler to resolve the instantiated branches.
      // Reading the original AST here would lose substitutions and private aliases.
      checker.typeToTypeNode(type, undefined, formatFlags);
      bindParameters(type.root.inferTypeParameters);
      if (!type.resolvedTrueType || !type.resolvedFalseType) return incomplete('unresolved conditional branches', type);
      return { Conditional: { check: shape(type.checkType, depth + 1), extends: shape(type.extendsType, depth + 1),
        then_type: shape(type.resolvedTrueType, depth + 1), else_type: shape(type.resolvedFalseType, depth + 1) } };
    }
    if (flags & ts.TypeFlags.IndexedAccess) return { Indexed: { base: shape(type.objectType, depth + 1), index: shape(type.indexType, depth + 1) } };
    if (flags & ts.TypeFlags.Index) return generic('keyof', [shape(type.type, depth + 1)]);
    if (flags & ts.TypeFlags.UniqueESSymbol) return { TypeParam: checker.typeToString(type) };
    if (!(flags & ts.TypeFlags.Object)) return incomplete('unevaluated compiler type', type);

    const node = checker.typeToTypeNode(type, undefined, formatFlags);
    if (node && ts.isMappedTypeNode(node)) {
      // These cached compiler types retain instantiation and alias expansion. Guard
      // their presence because MappedType's cache fields are not a public TS API.
      if (!type.typeParameter || !type.constraintType || !type.templateType) return incomplete('unresolved mapped type components', type);
      bindParameters([type.typeParameter]);
      const polarity = token => token ? token.kind !== ts.SyntaxKind.MinusToken : null;
      return { Mapped: { key_name: parameterNames.get(type.typeParameter), key_type: shape(type.constraintType, depth + 1),
        name_type: type.nameType ? shape(type.nameType, depth + 1) : null, value_type: shape(type.templateType, depth + 1),
        readonly: polarity(node.readonlyToken), optional: polarity(node.questionToken) } };
    }
    const readonlyProperties = new Map();
    if (node && ts.isTypeLiteralNode(node)) {
      for (const member of node.members) {
        if (member.name) {
          const name = member.name.text || printer.printNode(ts.EmitHint.Unspecified, member.name, program.getSourceFiles()[0]);
          const readonly = !!member.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.ReadonlyKeyword) ||
            ts.isGetAccessorDeclaration(member) && !node.members.some(other => ts.isSetAccessorDeclaration(other) && other.name?.text === member.name.text);
          readonlyProperties.set(name, readonly);
        }
      }
    }
    const fields = Object.create(null);
    for (const property of checker.getPropertiesOfType(type).sort((a, b) => propertyName(a).localeCompare(propertyName(b)))) {
      if (isPrivate(property)) continue;
      const declarations = property.declarations || [];
      const readonly = readonlyProperties.get(propertyName(property)) ?? (
        declarations.some(declaration => declaration.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.ReadonlyKeyword)) ||
        declarations.some(ts.isGetAccessorDeclaration) && !declarations.some(ts.isSetAccessorDeclaration));
      fields[propertyName(property)] = { optional: !!(property.flags & ts.SymbolFlags.Optional), readonly,
        shape: shape(checker.getTypeOfSymbol(property), depth + 1) };
    }
    const members = [];
    if (Object.keys(fields).length) members.push({ Object: fields });
    for (const index of checker.getIndexInfosOfType(type)) {
      const indexShape = generic('Record', [shape(index.keyType, depth + 1), shape(index.type, depth + 1)]);
      members.push(index.isReadonly ? generic('Readonly', [indexShape]) : indexShape);
    }
    for (const signature of checker.getSignaturesOfType(type, ts.SignatureKind.Call)) members.push(signatureShape(signature, depth));
    for (const signature of checker.getSignaturesOfType(type, ts.SignatureKind.Construct)) members.push(generic('<constructor>', [signatureShape(signature, depth)]));
    return members.length > 1 ? { Intersection: members } : members[0] || { Object: {} };
  } finally {
    active.delete(type);
  }
}
const fileIndexes = new Map();
function declarationSymbol(request) {
  const source = program.getSourceFile(request.file);
  if (!source) return undefined;
  let index = fileIndexes.get(source);
  if (!index) {
    index = new Map();
    function visit(node) {
      if (ts.isIdentifier(node) || ts.isStringLiteral(node)) {
        const name = node.text;
        if (!index.has(name)) index.set(name, []);
        index.get(name).push(node);
      }
      ts.forEachChild(node, visit);
    }
    visit(source);
    fileIndexes.set(source, index);
  }
  const line = Math.min(Math.max(0, request.line - 1), source.getLineStarts().length - 1);
  const lineStart = source.getLineStarts()[line];
  const nextLine = source.getLineStarts()[line + 1] || source.text.length;
  const column = Buffer.from(source.text.slice(lineStart, nextLine)).subarray(0, Math.max(0, request.col - 1)).toString('utf8').length;
  const position = lineStart + column;
  const candidates = index.get(request.local_name || request.name) || [];
  const nearest = [...candidates].sort((a, b) => Math.abs(a.getStart() - position) - Math.abs(b.getStart() - position));
  let symbol = nearest.length ? checker.getSymbolAtLocation(nearest[0]) : undefined;
  if (!symbol) {
    const module = checker.getSymbolAtLocation(source);
    symbol = module && checker.getExportsOfModule(module).find(candidate => candidate.getName() === request.name);
  }
  if (symbol?.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol);
  return symbol;
}
const results = [];
for (const request of input.symbols) {
  symbolWork = 4096;
  active = new Set();
  currentIssues = new Set();
  parameterNames = new Map();
  parameterScope = 0;
  const symbol = declarationSymbol(request);
  let overloads = [];
  if (symbol) {
    for (const declaration of symbol.declarations || []) {
      bindParameters(declaration.typeParameters?.map(parameter => checker.getTypeAtLocation(parameter)));
    }
    const type = symbol.flags & (ts.SymbolFlags.TypeAlias | ts.SymbolFlags.Interface | ts.SymbolFlags.Class)
      ? checker.getDeclaredTypeOfSymbol(symbol) : checker.getTypeOfSymbol(symbol);
    const signatures = checker.getSignaturesOfType(type, ts.SignatureKind.Call);
    if (signatures.length && !checker.getPropertiesOfType(type).length) overloads = signatures.map(signature => signatureShape(signature, 0));
    else overloads = [shape(type)];
  } else currentIssues.add('compiler could not locate public declaration');
  results.push({ id: request.id, overloads, issues: [...currentIssues] });
  if (packageWork < 0) {
    issues.add('compiler package expansion budget exhausted');
    break;
  }
}
const output = JSON.stringify({ results, issues: [...issues] });
if (Buffer.byteLength(output) > 16 * 1024 * 1024) {
  process.stdout.write(JSON.stringify({ results: [], issues: ['compiler output budget exhausted'] }));
} else process.stdout.write(output);
