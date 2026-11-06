// Resolve public contracts with the same compiler installation as the language server.
// No package code is executed. Keep the wire format aligned with TypeShape's serde format.
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
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
// Public contracts retain nullability and explicit undefined even under looser author settings.
Object.assign(options, { allowJs: true, checkJs: true, noEmit: true, skipLibCheck: true,
  strictNullChecks: true, strictFunctionTypes: true, exactOptionalPropertyTypes: true });
const program = ts.createProgram([...new Set(input.symbols.map(symbol => symbol.file))], options);
for (const diagnostic of program.getSyntacticDiagnostics()) {
  const source = diagnostic.file;
  const position = source && diagnostic.start !== undefined
    ? source.getLineAndCharacterOfPosition(diagnostic.start) : null;
  const location = source ? `${path.relative(input.root, source.fileName)}${position ? `:${position.line + 1}:${position.character + 1}` : ''}: ` : '';
  issues.add(`${location}TS${diagnostic.code}: ${ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ')}`);
}
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
let accessorMappers;
// These IDs have meaning only within one program. Comparison preserves relations
// between public uses instead of treating private declaration names as API names.
const nominalDeclarations = new Map();
function nominalIdentity(declaration) {
  if (!nominalDeclarations.has(declaration)) nominalDeclarations.set(declaration, String(nominalDeclarations.size));
  return { Literal: { String: nominalDeclarations.get(declaration) } };
}
function nominalShape(identities, body) {
  return identities.length ? generic('<nominal>', [...identities, body]) : body;
}
function mappedParameter(type, mapper, depth = 0) {
  if (!mapper) return type;
  if (depth > 32) { currentIssues.add('accessor type substitution limit reached'); return type; }
  switch (mapper.kind) {
    case 0: return type === mapper.source ? mapper.target : type;
    case 1:
    case 2: {
      const index = mapper.sources.indexOf(type);
      if (index < 0) return type;
      return mapper.kind === 2 ? mapper.targets[index]() : mapper.targets?.[index] || checker.getAnyType();
    }
    case 3: return mapper.func(type);
    case 4:
    case 5: return mappedParameter(mappedParameter(type, mapper.mapper1, depth + 1), mapper.mapper2, depth + 1);
    default: currentIssues.add('unsupported accessor type substitution'); return type;
  }
}
function accessorWriteShape(property, setter, depth) {
  if (!setter?.parameters[0]) return null;
  // Instantiated accessor symbols carry the same substitutions used for their
  // read type. Preserve them when serializing the separate setter parameter.
  const mapper = property.links?.mapper;
  if (mapper) accessorMappers.push(mapper);
  try { return shape(property.links?.writeType || checker.getTypeAtLocation(setter.parameters[0]), depth); }
  finally { if (mapper) accessorMappers.pop(); }
}
function propertyShape(property, depth, readonlyOverride) {
  const declarations = property.declarations || [];
  const readonly = readonlyOverride ?? (
    declarations.some(declaration => declaration.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.ReadonlyKeyword)) ||
    declarations.some(ts.isGetAccessorDeclaration) && !declarations.some(ts.isSetAccessorDeclaration));
  return { optional: !!(property.flags & ts.SymbolFlags.Optional), readonly,
    write_type: accessorWriteShape(property, declarations.find(ts.isSetAccessorDeclaration), depth),
    // Declaration locations retain explicit undefined without the absent-property marker.
    shape: shape(checker.getTypeOfSymbolAtLocation(property, declarations[0] || program.getSourceFiles()[0]), depth) };
}
function bindParameters(parameters) {
  const unbound = (parameters || []).filter(parameter => !parameterNames.has(parameter));
  if (!unbound.length) return;
  const scope = parameterScope++;
  unbound.forEach((parameter, index) => parameterNames.set(parameter, `T${scope}_${index}`));
}
function withParameters(parameters, resolve) {
  const previousNames = new Map(parameterNames), previousScope = parameterScope;
  bindParameters(parameters);
  try { return resolve(); }
  finally { parameterNames = previousNames; parameterScope = previousScope; }
}
function incomplete(message, type) {
  currentIssues.add(message);
  return { Opaque: type ? checker.typeToString(type) : '<unresolved>' };
}
function signatureShape(signature, depth, classConstructor = false) {
  return withParameters(signature.typeParameters, () => {
  const predicate = checker.getTypePredicateOfSignature(signature);
  const body = { Function: {
    params: signature.getParameters().map(parameter => {
      const declaration = parameter.valueDeclaration || parameter.declarations?.[0];
      return { name: parameter.getName(),
        optional: !!(parameter.flags & ts.SymbolFlags.Optional) || !!declaration?.questionToken || !!declaration?.initializer,
        rest: !!declaration?.dotDotDotToken,
        shape: shape(checker.getTypeOfSymbol(parameter), depth + 1) };
    }),
    return_type: classConstructor ? primitive('Void') : shape(checker.getReturnTypeOfSignature(signature), depth + 1),
    contract: {
      receiver: signature.thisParameter ? shape(checker.getTypeOfSymbol(signature.thisParameter), depth + 1) : null,
      predicate: predicate ? {
        target: predicate.kind === ts.TypePredicateKind.This || predicate.kind === ts.TypePredicateKind.AssertsThis
          ? 'Receiver' : { Parameter: predicate.parameterIndex },
        asserts: predicate.kind === ts.TypePredicateKind.AssertsThis || predicate.kind === ts.TypePredicateKind.AssertsIdentifier,
        shape: predicate.type ? shape(predicate.type, depth + 1) : null,
      } : null,
    },
  } };
  return genericDeclaration(signature.typeParameters, body, depth);
  });
}

function genericDeclaration(parameters, body, depth) {
  if (!parameters?.length) return body;
  return { GenericDeclaration: { params: parameters.map(parameter => {
    const declaration = parameter.symbol?.declarations?.find(ts.isTypeParameterDeclaration);
    // Ask the checker to initialize its immediate constraint cache; the base
    // constraint alone would erase U extends T when T also has a constraint.
    const baseConstraint = checker.getBaseConstraintOfType(parameter);
    const constraint = declaration?.constraint ? parameter.constraint || baseConstraint || checker.getTypeFromTypeNode(declaration.constraint) : null;
    const defaultType = checker.getDefaultFromTypeParameter(parameter);
    const modifiers = declaration?.modifiers || [];
    const has = kind => modifiers.some(modifier => modifier.kind === kind);
    const inward = has(ts.SyntaxKind.InKeyword), outward = has(ts.SyntaxKind.OutKeyword);
    return { constraint: constraint ? shape(constraint, depth + 1) : null,
      default: defaultType ? shape(defaultType, depth + 1) : null,
      is_const: has(ts.SyntaxKind.ConstKeyword), variance: inward && outward ? 'InOut' : inward ? 'In' : outward ? 'Out' : null };
  }), body } };
}
function constantScalar(expression, depth = 0) {
  if (!expression || depth > 32) return undefined;
  const type = checker.getTypeAtLocation(expression);
  if (type.flags & (ts.TypeFlags.StringLiteral | ts.TypeFlags.NumberLiteral)) return type.value;
  if (type.flags & ts.TypeFlags.BooleanLiteral) return type.intrinsicName === 'true';
  if (type.flags & ts.TypeFlags.Null) return null;
  if (ts.isParenthesizedExpression(expression) || ts.isAsExpression(expression) || ts.isSatisfiesExpression(expression)) return constantScalar(expression.expression, depth + 1);
  if (ts.isIdentifier(expression)) return constantScalar(constantInitializer(expression), depth + 1);
  if (ts.isBinaryExpression(expression) && expression.operatorToken.kind === ts.SyntaxKind.PlusToken) {
    const left = constantScalar(expression.left, depth + 1), right = constantScalar(expression.right, depth + 1);
    if (left !== undefined && right !== undefined) return left + right;
  }
  if (ts.isTemplateExpression(expression)) {
    let text = expression.head.text;
    for (const span of expression.templateSpans) {
      const part = constantScalar(span.expression, depth + 1);
      if (part === undefined) return undefined;
      text += String(part) + span.literal.text;
    }
    return text;
  }
  return undefined;
}
function constantInitializer(expression) {
  const declaration = checker.getSymbolAtLocation(expression)?.valueDeclaration;
  return declaration && ts.isVariableDeclaration(declaration) && (declaration.parent.flags & ts.NodeFlags.Const) ? declaration.initializer : undefined;
}
function symbolIdentity(expression, depth = 0) {
  if (!expression || depth > 32) return undefined;
  if (ts.isParenthesizedExpression(expression) || ts.isAsExpression(expression) || ts.isSatisfiesExpression(expression)) return symbolIdentity(expression.expression, depth + 1);
  if (ts.isIdentifier(expression)) return symbolIdentity(constantInitializer(expression), depth + 1);
  const member = ts.isCallExpression(expression) ? expression.expression : expression;
  if (!ts.isPropertyAccessExpression(member) || !ts.isIdentifier(member.expression) || member.expression.text !== 'Symbol') return undefined;
  const owner = checker.getSymbolAtLocation(member.expression);
  if (!owner?.declarations?.every(declaration => program.isSourceFileDefaultLibrary(declaration.getSourceFile()))) return undefined;
  if (!ts.isCallExpression(expression)) return generic('Symbol', [{ Literal: { String: member.name.text } }]);
  if (member.name.text !== 'for') return undefined;
  const key = constantScalar(expression.arguments[0]);
  if (key === undefined) {
    currentIssues.add('unresolved Symbol.for registry key');
    return undefined;
  }
  return generic('Symbol.for', [{ Literal: { String: String(key) } }]);
}
function propertyName(symbol) {
  const declaration = symbol.valueDeclaration || symbol.declarations?.[0];
  if (!symbol.getName().startsWith('__@') || !declaration?.name || !ts.isComputedPropertyName(declaration.name)) return symbol.getName();
  const identity = symbolIdentity(declaration.name.expression)?.Generic;
  if (identity) return identity.name === 'Symbol.for'
    ? `[Symbol.for(${JSON.stringify(identity.args[0].Literal.String)})]`
    : `[Symbol.${identity.args[0].Literal.String}]`;
  return declaration.name.getText();
}
function commonjsSymbolName(identity) {
  const symbol = identity?.Generic;
  if (!symbol) return undefined;
  return symbol.name === 'Symbol.for'
    ? `@@for(${JSON.stringify(symbol.args[0].Literal.String)})`
    : `@@${symbol.args[0].Literal.String}`;
}
function commonjsMembers(type, serialized) {
  if (!(type.flags & ts.TypeFlags.Object) || type.symbol?.flags & ts.SymbolFlags.Class ||
      checker.getSignaturesOfType(type, ts.SignatureKind.Call).length ||
      checker.getSignaturesOfType(type, ts.SignatureKind.Construct).length) return null;
  const indexes = checker.getIndexInfosOfType(type);
  if (!serialized.Object && !indexes.length) return null;
  if (indexes.some(index => !(index.keyType.flags & ts.TypeFlags.ESSymbol))) return null;

  const properties = checker.getPropertiesOfType(type);
  if (properties.length > 4096 || symbolWork < 0) {
    currentIssues.add('compiler public type expansion limit reached');
    return null;
  }
  const typeNode = checker.typeToTypeNode(type, undefined, formatFlags);
  const members = new Map();
  function add(property, computedName) {
    const declaration = property.valueDeclaration || property.declarations?.[0];
    const name = computedName || declaration?.name;
    const symbolKey = name && ts.isComputedPropertyName(name) &&
      checker.getTypeAtLocation(name.expression).flags & (ts.TypeFlags.ESSymbol | ts.TypeFlags.UniqueESSymbol);
    const key = symbolKey ? commonjsSymbolName(symbolIdentity(name.expression)) : property.getName();
    if (key === undefined) {
      currentIssues.add('CommonJS symbol property identity requires runtime evidence');
      return false;
    }
    let readonly;
    if (typeNode && ts.isTypeLiteralNode(typeNode)) {
      const declarationName = propertyName(property);
      const matching = typeNode.members.filter(member => member.name &&
        ts.isComputedPropertyName(member.name) === !!symbolKey &&
        (member.name.text ?? printer.printNode(ts.EmitHint.Unspecified, member.name, program.getSourceFiles()[0])) === declarationName);
      if (matching.length) readonly = matching.some(member =>
        member.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.ReadonlyKeyword)) ||
        matching.some(ts.isGetAccessorDeclaration) && !matching.some(ts.isSetAccessorDeclaration);
    }
    const field = propertyShape(property, 1, readonly);
    members.set(JSON.stringify([!!symbolKey, key]), { name: key, symbol: !!symbolKey, field });
    return true;
  }
  for (const property of properties) {
    if (!add(property)) return null;
  }
  if (indexes.length) {
    // The checker widens Symbol.for keys to a symbol index. Recover only keys
    // from the exact inferred object literal; an arbitrary index has no finite
    // public-key inventory and must remain missing evidence.
    const declarations = type.symbol?.declarations || [];
    if (declarations.length !== 1 || !ts.isObjectLiteralExpression(declarations[0]) ||
        declarations[0].properties.length > 4096) {
      currentIssues.add('CommonJS symbol index requires concrete property identities');
      return null;
    }
    let recovered = false;
    for (const member of declarations[0].properties) {
      if (ts.isSpreadAssignment(member)) {
        currentIssues.add('CommonJS spread symbol index requires concrete property identities');
        return null;
      }
      if (!member.name || !ts.isComputedPropertyName(member.name)) continue;
      const keyType = checker.getTypeAtLocation(member.name.expression);
      if (!(keyType.flags & (ts.TypeFlags.ESSymbol | ts.TypeFlags.UniqueESSymbol))) continue;
      const property = checker.getSymbolAtLocation(member.name);
      if (!property || ts.isGetAccessorDeclaration(member) || ts.isSetAccessorDeclaration(member)) {
        currentIssues.add('CommonJS computed symbol member requires concrete property evidence');
        return null;
      }
      if (!add(property, member.name)) return null;
      recovered = true;
    }
    if (!recovered) {
      currentIssues.add('CommonJS symbol index requires concrete property identities');
      return null;
    }
  }
  return [...members.values()];
}
function isPrivate(symbol) {
  return symbol?.declarations?.some(declaration =>
    declaration.name && ts.isPrivateIdentifier(declaration.name) ||
    declaration.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.PrivateKeyword));
}
function protectedOrigin(type, name, fallback, seen = new Set()) {
  const target = type.target || type;
  if (seen.has(target)) return fallback;
  seen.add(target);
  for (const base of target.getBaseTypes?.() || []) {
    const property = checker.getPropertyOfType(base, name);
    const declaration = property?.declarations?.find(declaration =>
      declaration.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.ProtectedKeyword));
    if (declaration) return protectedOrigin(base, name, declaration, seen);
  }
  return fallback;
}
function shape(type, depth = 0, staticClass = null) {
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
    let instantiated = type;
    for (const mapper of [...accessorMappers].reverse()) instantiated = mappedParameter(instantiated, mapper);
    if (instantiated !== type) return shape(instantiated, depth + 1);
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
      const tuple = { Tuple: checker.getTypeArguments(type).map((element, index) => {
        const elementFlags = target.elementFlags[index];
        const rest = !!(elementFlags & (ts.ElementFlags.Rest | ts.ElementFlags.Variadic));
        let elementShape = shape(element, depth + 1);
        if (elementFlags & ts.ElementFlags.Rest) elementShape = { Array: elementShape };
        return { label: target.labeledElementDeclarations?.[index]?.name?.getText() || null,
          optional: !!(elementFlags & ts.ElementFlags.Optional), rest, shape: elementShape };
      }) };
      return target.readonly ? generic('Readonly', [tuple]) : tuple;
    }
    if (checker.isArrayType(type)) {
      const element = shape(checker.getTypeArguments(type)[0], depth + 1);
      return type.symbol?.getName() === 'ReadonlyArray' ? generic('ReadonlyArray', [element]) : { Array: element };
    }
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
      if (!type.resolvedTrueType || !type.resolvedFalseType) return incomplete('unresolved conditional branches', type);
      return withParameters(type.root.inferTypeParameters, () => ({ Conditional: { check: shape(type.checkType, depth + 1), extends: shape(type.extendsType, depth + 1),
        then_type: shape(type.resolvedTrueType, depth + 1), else_type: shape(type.resolvedFalseType, depth + 1) } }));
    }
    if (flags & ts.TypeFlags.IndexedAccess) return { Indexed: { base: shape(type.objectType, depth + 1), index: shape(type.indexType, depth + 1) } };
    if (flags & ts.TypeFlags.Index) return generic('keyof', [shape(type.type, depth + 1)]);
    if (flags & ts.TypeFlags.UniqueESSymbol) return symbolIdentity(type.symbol?.valueDeclaration?.initializer) || nominalShape([nominalIdentity(type.symbol?.valueDeclaration || type.symbol)], primitive('Symbol'));
    if (!(flags & ts.TypeFlags.Object)) return incomplete('unevaluated compiler type', type);

    const node = checker.typeToTypeNode(type, undefined, formatFlags);
    if (node && ts.isMappedTypeNode(node)) {
      // These cached compiler types retain instantiation and alias expansion. Guard
      // their presence because MappedType's cache fields are not a public TS API.
      if (!type.typeParameter || !type.constraintType || !type.templateType) return incomplete('unresolved mapped type components', type);
      const polarity = token => token ? token.kind !== ts.SyntaxKind.MinusToken : null;
      return withParameters([type.typeParameter], () => ({ Mapped: { key_name: parameterNames.get(type.typeParameter), key_type: shape(type.constraintType, depth + 1),
        name_type: type.nameType ? shape(type.nameType, depth + 1) : null, value_type: shape(type.templateType, depth + 1),
        readonly: polarity(node.readonlyToken), optional: polarity(node.questionToken) } }));
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
    const brands = [];
    for (const property of checker.getPropertiesOfType(type).sort((a, b) => propertyName(a).localeCompare(propertyName(b)))) {
      for (const declaration of property.declarations || []) {
        if (declaration.name && ts.isPrivateIdentifier(declaration.name) ||
            declaration.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.PrivateKeyword || modifier.kind === ts.SyntaxKind.ProtectedKeyword)) {
          const origin = declaration.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.ProtectedKeyword)
            ? protectedOrigin(type, property.getName(), declaration) : declaration;
          brands.push(nominalIdentity(origin));
        }
      }
      if (isPrivate(property) || staticClass && property.getName() === 'prototype') continue;
      fields[propertyName(property)] = propertyShape(property, depth + 1, readonlyProperties.get(propertyName(property)));
    }
    const classDeclaration = staticClass || (type.objectFlags & ts.ObjectFlags.Anonymous && type.symbol?.flags & ts.SymbolFlags.Class
      ? type.symbol.declarations?.find(ts.isClassLike) : null);
    if (classDeclaration) applyStaticInitialization(classDeclaration, fields, depth);
    const members = [];
    if (Object.keys(fields).length) members.push({ Object: fields });
    for (const index of checker.getIndexInfosOfType(type)) {
      const indexShape = generic('Record', [shape(index.keyType, depth + 1), shape(index.type, depth + 1)]);
      members.push(index.isReadonly ? generic('Readonly', [indexShape]) : indexShape);
    }
    for (const signature of checker.getSignaturesOfType(type, ts.SignatureKind.Call)) members.push(signatureShape(signature, depth));
    for (const signature of checker.getSignaturesOfType(type, ts.SignatureKind.Construct)) members.push(generic('<constructor>', [signatureShape(signature, depth, !!staticClass)]));
    return nominalShape(brands, members.length > 1 ? { Intersection: members } : members[0] || { Object: {} });
  } finally {
    active.delete(type);
  }
}
function applyStaticInitialization(declaration, fields, depth) {
  function keyOf(expression) {
    const identity = symbolIdentity(expression)?.Generic;
    if (identity) return identity.name === 'Symbol.for' ? `[Symbol.for(${JSON.stringify(identity.args[0].Literal.String)})]` : `[Symbol.${identity.args[0].Literal.String}]`;
    const key = constantScalar(expression);
    return key === undefined ? undefined : String(key);
  }
  const declaredKeys = new Set();
  for (const member of declaration.members) {
    if (!member.name || !member.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.StaticKeyword)) continue;
    if (isPrivate(member.symbol) || ts.isPropertyDeclaration(member) &&
        (ts.getEffectiveTypeAnnotationNode(member) || member.modifiers.some(modifier => modifier.kind === ts.SyntaxKind.ReadonlyKeyword))) {
      declaredKeys.add(ts.isComputedPropertyName(member.name) ? keyOf(member.name.expression) : member.name.text);
    }
  }
  function memberPath(expression) {
    if (ts.isParenthesizedExpression(expression) || ts.isAsExpression(expression) || ts.isSatisfiesExpression(expression)) return memberPath(expression.expression);
    if (expression.kind === ts.SyntaxKind.ThisKeyword) return [];
    if (ts.isIdentifier(expression) && checker.getSymbolAtLocation(expression) === declaration.symbol) return [];
    if (ts.isPropertyAccessExpression(expression) || ts.isElementAccessExpression(expression)) {
      const parent = memberPath(expression.expression);
      if (!parent) return undefined;
      const key = ts.isPropertyAccessExpression(expression) ? expression.name.text : keyOf(expression.argumentExpression);
      return [...parent, key];
    }
    return undefined;
  }
  function assign(path, expression) {
    if (!path?.length) return;
    if (declaredKeys.has(path[0])) return;
    if (path.some(key => key === undefined)) { currentIssues.add('unresolved class static property key'); return; }
    let target = fields;
    for (const key of path.slice(0, -1)) {
      const nested = target[key]?.shape.Object;
      if (!nested) { currentIssues.add('unresolved class static property path'); return; }
      target = nested;
    }
    target[path.at(-1)] = { optional: false, readonly: false,
      shape: expression ? shape(checker.getBaseTypeOfLiteralType(checker.getTypeAtLocation(expression)), depth + 1) : primitive('Undefined') };
  }
  function visit(statement, conditional = false) {
    if (ts.isFunctionLike(statement) || ts.isClassLike(statement)) return;
    if (ts.isIfStatement(statement)) {
      const condition = constantScalar(statement.expression);
      if (condition !== undefined) visit(condition ? statement.thenStatement : statement.elseStatement || ts.factory.createEmptyStatement(), conditional);
      else { visit(statement.thenStatement, true); if (statement.elseStatement) visit(statement.elseStatement, true); }
      return;
    }
    if (ts.isBinaryExpression(statement) && statement.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
      const path = memberPath(statement.left);
      if (path?.length) {
        if (conditional) currentIssues.add('conditional class static assignment requires runtime evidence');
        else assign(path, statement.right);
        return;
      }
    }
    if (ts.isCallExpression(statement) && (memberPath(statement.expression)?.length || statement.arguments.some(argument => memberPath(argument)?.length === 0))) currentIssues.add('class static initialization calls require runtime evidence');
    const uncertain = conditional || !(ts.isBlock(statement) || ts.isExpressionStatement(statement));
    ts.forEachChild(statement, child => visit(child, uncertain));
  }
  for (const member of declaration.members) {
    if (ts.isClassStaticBlockDeclaration(member)) visit(member.body);
    else if (ts.isPropertyDeclaration(member) && member.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.StaticKeyword) && !isPrivate(member.symbol) && !member.type && !member.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.DeclareKeyword)) {
      const key = ts.isComputedPropertyName(member.name) ? keyOf(member.name.expression) : member.name.text;
      assign([key], member.initializer);
    }
  }
}
const fileIndexes = new Map();
function sourcePosition(source, request) {
  const line = Math.min(Math.max(0, request.line - 1), source.getLineStarts().length - 1);
  const start = source.getLineStarts()[line];
  const end = source.getLineStarts()[line + 1] || source.text.length;
  return start + Buffer.from(source.text.slice(start, end)).subarray(0, Math.max(0, request.col - 1)).toString('utf8').length;
}

function commonjsValue(request) {
  const source = program.getSourceFile(request.file);
  if (!source) return undefined;
  const position = sourcePosition(source, request);
  let expression;
  let initializer;
  let moduleExport = false;
  let explicitAny = false;
  let property;
  const contains = node => node.getStart() <= position && position < node.getEnd();
  const propertyName = node => ts.isPropertyAccessExpression(node) ? node.name.text
    : ts.isElementAccessExpression(node) && node.argumentExpression && ts.isStringLiteralLike(node.argumentExpression) ? node.argumentExpression.text : undefined;
  const isModuleExports = node => (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node))
    && ts.isIdentifier(node.expression) && node.expression.text === 'module' && propertyName(node) === 'exports';
  const isExportsObject = node => ts.isIdentifier(node) && node.text === 'exports' || isModuleExports(node);
  function isExportProperty(node) {
    while (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) {
      if (isExportsObject(node.expression)) return true;
      node = node.expression;
    }
    return false;
  }
  function visit(node) {
    if (!contains(node)) return;
    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
      if (isModuleExports(node.left) && request.export_kind === 'default') {
        expression = node.right;
        let result = node.right;
        while (ts.isParenthesizedExpression(result)) result = result.expression;
        moduleExport = true;
        if (ts.isIdentifier(result)) {
          const declaration = checker.getSymbolAtLocation(result)?.valueDeclaration;
          const annotation = declaration && (declaration.type || ts.getJSDocType(declaration));
          explicitAny = !!annotation && !!(checker.getTypeFromTypeNode(annotation).flags & ts.TypeFlags.Any);
        }
      } else if (isExportProperty(node.left) && contains(node.left)) {
        // A public property annotation can widen the fresh RHS literal. Keep
        // the RHS only when the checker cannot resolve the key.
        const type = checker.getTypeAtLocation(node.left);
        expression = type.flags & ts.TypeFlags.Any ? node.right : node.left;
        property = checker.getSymbolAtLocation(node.left);
      } else if (isModuleExports(node.left) && request.export_kind === 'named' && ts.isObjectLiteralExpression(node.right)) {
        const member = node.right.properties.find(member => member.name && contains(member.name));
        if (member) property = checker.getSymbolAtLocation(member.name);
      }
      if (expression) initializer = node.right;
    }
    if (ts.isPropertyAssignment(node) && ts.isComputedPropertyName(node.name) && contains(node.name)) expression = node.initializer;
    ts.forEachChild(node, visit);
  }
  visit(source);
  if (!expression && !property) return undefined;
  return { type: expression ? checker.getTypeAtLocation(expression) : checker.getTypeOfSymbol(property),
    identity: symbolIdentity(initializer || expression), moduleExport, explicitAny, property };
}

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
  const position = sourcePosition(source, request);
  const candidates = index.get(request.local_name || request.name) || [];
  const nearest = [...candidates].sort((a, b) => Math.abs(a.getStart() - position) - Math.abs(b.getStart() - position));
  let symbol = nearest.length ? checker.getSymbolAtLocation(nearest[0]) : undefined;
  if (!symbol) {
    const module = checker.getSymbolAtLocation(source);
    const exports = module ? checker.getExportsOfModule(module) : [];
    // Namespace reexports carry a synthetic public name such as ns.default;
    // its source module still exports the member under the local name.
    symbol = exports.find(candidate => candidate.getName() === (request.local_name || request.name))
      || exports.find(candidate => candidate.getName() === request.name);
  }
  if (!symbol && request.name === 'default') {
    // Discovery represents TypeScript's `export =` as a default export, while
    // the checker exposes its target under the original binding's name.
    const assignment = source.statements.find(statement => ts.isExportAssignment(statement) && statement.isExportEquals);
    if (assignment) symbol = checker.getSymbolAtLocation(assignment.expression);
  }
  if (symbol?.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol);
  return symbol;
}

function definitionLocation(request) {
  const symbol = declarationSymbol(request);
  const declaration = symbol?.valueDeclaration || symbol?.declarations?.[0];
  if (!declaration) return null;
  const target = declaration.name || declaration;
  const source = target.getSourceFile();
  return { uri: pathToFileURL(source.fileName).href, range: {
    start: source.getLineAndCharacterOfPosition(target.getStart()),
    end: source.getLineAndCharacterOfPosition(target.getEnd()),
  } };
}
const results = [];
for (const request of input.symbols) {
  symbolWork = 4096;
  active = new Set();
  currentIssues = new Set();
  parameterNames = new Map();
  parameterScope = 0;
  accessorMappers = [];
  const symbol = declarationSymbol(request);
  let overloads = [];
  let signature = null;
  let commonjsMemberList = null;
  let commonjsProperty = null;
  const expressionValue = commonjsValue(request);
  if (symbol || expressionValue) {
    if (symbol?.flags & (ts.SymbolFlags.TypeAlias | ts.SymbolFlags.Interface | ts.SymbolFlags.Class)) {
      for (const declaration of symbol.declarations || []) {
        bindParameters(declaration.typeParameters?.map(parameter => checker.getTypeAtLocation(parameter)));
      }
    }
    const type = expressionValue?.type || (symbol.flags & (ts.SymbolFlags.TypeAlias | ts.SymbolFlags.Interface | ts.SymbolFlags.Class)
      ? checker.getDeclaredTypeOfSymbol(symbol) : checker.getTypeOfSymbol(symbol));
    if (expressionValue?.moduleExport && !expressionValue.explicitAny && type.flags & (ts.TypeFlags.Any | ts.TypeFlags.Unknown)) {
      currentIssues.add('CommonJS export result requires runtime evidence');
    }
    signature = checker.typeToString(type, undefined, ts.TypeFormatFlags.NoTruncation);
    const signatures = checker.getSignaturesOfType(type, ts.SignatureKind.Call);
    if (signatures.length && !checker.getPropertiesOfType(type).length) overloads = signatures.map(signature => signatureShape(signature, 0));
    else overloads = [expressionValue?.identity || symbolIdentity(symbol?.valueDeclaration?.initializer) || shape(type)];
    if (symbol?.flags & ts.SymbolFlags.Class) {
      const declaration = symbol.declarations?.find(ts.isClassLike);
      if (declaration) overloads = overloads.map(instance => ({ Intersection: [instance,
        generic('<class-static>', [shape(checker.getTypeOfSymbol(symbol), 0, declaration)])] }));
    }
    if (symbol?.flags & (ts.SymbolFlags.TypeAlias | ts.SymbolFlags.Interface | ts.SymbolFlags.Class)) {
      const declaration = symbol.declarations?.find(declaration => declaration.typeParameters?.length);
      const parameters = declaration?.typeParameters.map(parameter => checker.getTypeAtLocation(parameter));
      overloads = overloads.map(body => genericDeclaration(parameters, body, 0));
    }
    if (expressionValue?.moduleExport && overloads.length === 1) {
      commonjsMemberList = commonjsMembers(type, overloads[0]);
    }
    if (expressionValue?.property) commonjsProperty = propertyShape(expressionValue.property, 0);
  } else currentIssues.add('compiler could not locate public declaration');
  const typeof_targets = (request.refs || []).map(reference => definitionLocation({ ...reference, file: request.file }));
  results.push({ id: request.id, overloads, signature, commonjs_members: commonjsMemberList, commonjs_property: commonjsProperty, typeof_targets, issues: [...currentIssues] });
  if (packageWork < 0) {
    issues.add('compiler package expansion budget exhausted');
    break;
  }
}
const output = JSON.stringify({ results, issues: [...issues] });
if (Buffer.byteLength(output) > 16 * 1024 * 1024) {
  process.stdout.write(JSON.stringify({ results: [], issues: ['compiler output budget exhausted'] }));
} else process.stdout.write(output);
