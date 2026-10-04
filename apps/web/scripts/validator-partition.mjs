// QuaZonai adapter around official AJV standalone output. This does not implement
// validation: the pinned TypeScript binder only relocates native declarations.
import ts from 'typescript';

const fail = message => { throw new Error(`Unsupported AJV standalone program: ${message}`); };
const printer = ts.createPrinter({ newLine: ts.NewLineKind.LineFeed, removeComments: true });

function bind(source) {
  const filename = '/native-standalone.js';
  const options = { allowJs: true, noResolve: true, noLib: true, target: ts.ScriptTarget.ES2022 };
  const host = ts.createCompilerHost(options);
  host.getSourceFile = name => name === filename
    ? ts.createSourceFile(filename, source, options.target, true, ts.ScriptKind.JS) : undefined;
  host.fileExists = name => name === filename;
  host.readFile = name => name === filename ? source : undefined;
  const program = ts.createProgram([filename], options, host);
  const ast = program.getSourceFile(filename);
  if (program.getSyntacticDiagnostics(ast).length) fail('invalid syntax');
  return { ast, checker: program.getTypeChecker() };
}

const propertyName = node => ts.isIdentifier(node) || ts.isStringLiteral(node) || ts.isNumericLiteral(node) ? node.text : undefined;
function literal(node, allowUndefined = false) {
  if (ts.isStringLiteral(node) || ts.isNumericLiteral(node)
    || [ts.SyntaxKind.NullKeyword, ts.SyntaxKind.TrueKeyword, ts.SyntaxKind.FalseKeyword].includes(node.kind)) return true;
  if (allowUndefined && ts.isIdentifier(node) && node.text === 'undefined') return true;
  if (ts.isPrefixUnaryExpression(node) && node.operator === ts.SyntaxKind.MinusToken && ts.isNumericLiteral(node.operand)) return true;
  if (ts.isArrayLiteralExpression(node)) return node.elements.every(value => literal(value, allowUndefined));
  return ts.isObjectLiteralExpression(node) && node.properties.every(property => ts.isPropertyAssignment(property)
    && propertyName(property.name) !== undefined && literal(property.initializer, allowUndefined));
}
function accessPath(node) {
  if (ts.isIdentifier(node)) return [node.text];
  if (ts.isPropertyAccessExpression(node)) { const base = accessPath(node.expression); return base && [...base, node.name.text]; }
  if (ts.isElementAccessExpression(node) && ts.isStringLiteral(node.argumentExpression)) {
    const base = accessPath(node.expression); return base && [...base, node.argumentExpression.text];
  }
  if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === 'require'
    && node.arguments.length === 1 && ts.isStringLiteral(node.arguments[0])) return ['require', node.arguments[0].text];
}
function pureInitializer(node, functions, checker) {
  if (literal(node) || ts.isRegularExpressionLiteral(node)) return 'literal';
  if (ts.isNewExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === 'RegExp'
    && !checker.getSymbolAtLocation(node.expression) && node.arguments?.length === 2
    && node.arguments.every(ts.isStringLiteral)) {
    // Also reject invalid flags/patterns before any output can be published.
    new RegExp(node.arguments[0].text, node.arguments[1].text);
    return 'regexp';
  }
  const access = accessPath(node);
  if (access?.join('.') === 'Object.prototype.hasOwnProperty') return 'runtime';
  if (access?.length === 3 && access[0] === 'require' && access[2] === 'default'
    && ['ajv/dist/runtime/ucs2length', 'ajv/dist/runtime/equal'].includes(access[1])) return 'runtime';
  if (access?.length === 4 && access[0] === 'require' && access[1] === 'ajv-formats/dist/formats'
    && access[2] === 'fullFormats' && /^[a-z][a-z0-9-]*$/.test(access[3])) return 'runtime';
  if (ts.isObjectLiteralExpression(node) && node.properties.length === 1) {
    const property = node.properties[0];
    if (ts.isPropertyAssignment(property) && propertyName(property.name) === 'validate'
      && ts.isIdentifier(property.initializer) && functions.has(checker.getSymbolAtLocation(property.initializer))) return 'wrapper';
  }
  fail(`unrecognized constant initializer ${node.getText().slice(0, 100)}`);
}
function namePosition(node) {
  const parent = node.parent;
  return (ts.isPropertyAccessExpression(parent) && parent.name === node)
    || (ts.isPropertyAssignment(parent) && parent.name === node)
    || (ts.isLabeledStatement(parent) && parent.label === node)
    || ((ts.isBreakStatement(parent) || ts.isContinueStatement(parent)) && parent.label === node);
}

export function partitionStandalone(source, provenance) {
  if (ts.version !== '5.9.3') fail('requires pinned TypeScript 5.9.3');
  if (!(provenance instanceof Map)) fail('provenance must be a Map');
  const { ast, checker } = bind(source);
  const referenceSymbol = node => ts.isIdentifier(node) && ts.isShorthandPropertyAssignment(node.parent)
    ? checker.getShorthandAssignmentValueSymbol(node.parent) : checker.getSymbolAtLocation(node);
  const units = []; const symbols = new Map(); const names = new Map(); const nativeExports = new Map();
  for (const statement of ast.statements) {
    let name; let kind;
    if (ts.isFunctionDeclaration(statement)) {
      if (!statement.name || !statement.body || statement.modifiers?.length || statement.asteriskToken) fail('function shape');
      name = statement.name; kind = 'function';
    } else if (ts.isVariableStatement(statement)) {
      const declarations = statement.declarationList.declarations;
      if (statement.modifiers?.length || declarations.length !== 1 || !ts.isIdentifier(declarations[0].name)
        || (statement.declarationList.flags & ts.NodeFlags.BlockScoped) !== ts.NodeFlags.Const || !declarations[0].initializer) fail('constant shape');
      name = declarations[0].name; kind = 'constant';
    } else continue;
    const symbol = checker.getSymbolAtLocation(name);
    if (!symbol || names.has(name.text) || symbols.has(symbol)
      || ['exports', 'require', 'Object', 'RegExp', 'Array', 'undefined', 'isNaN'].includes(name.text)) fail(`duplicate/reserved binding ${name.text}`);
    const unit = { id: units.length, name: name.text, symbol, kind, statements: [statement], dependencies: new Set() };
    units.push(unit); names.set(name.text, unit); symbols.set(symbol, unit);
    if (kind === 'function') {
      unit.pointer = provenance.get(name.text);
      if (typeof unit.pointer !== 'string' || !(unit.pointer === '#' || unit.pointer.startsWith('#/')) || /~(?![01])/.test(unit.pointer)) fail(`missing schema provenance: ${name.text}`);
    }
  }
  const functions = new Set(units.filter(unit => unit.kind === 'function').map(unit => unit.symbol));
  for (const unit of units) if (unit.kind === 'constant') {
    unit.initializerKind = pureInitializer(unit.statements[0].declarationList.declarations[0].initializer, functions, checker);
  }
  for (const statement of ast.statements) {
    if (ts.isFunctionDeclaration(statement) || ts.isVariableStatement(statement)) continue;
    if (!ts.isExpressionStatement(statement)) fail('top-level statement');
    const expression = statement.expression;
    if (ts.isStringLiteral(expression) && expression.text === 'use strict') continue;
    if (!ts.isBinaryExpression(expression) || expression.operatorToken.kind !== ts.SyntaxKind.EqualsToken
      || !ts.isPropertyAccessExpression(expression.left) || !ts.isIdentifier(expression.left.expression)) fail('top-level effect');
    const object = expression.left.expression;
    if (object.text === 'exports') {
      const target = ts.isIdentifier(expression.right) && symbols.get(checker.getSymbolAtLocation(expression.right));
      const name = expression.left.name.text;
      if (!target || target.kind !== 'function' || nativeExports.has(name)) fail(`export ${name}`);
      nativeExports.set(name, target); continue;
    }
    const owner = symbols.get(checker.getSymbolAtLocation(object));
    if (!owner || owner.kind !== 'function' || expression.left.name.text !== 'evaluated'
      || owner.statements.length !== 1 || !ts.isObjectLiteralExpression(expression.right)
      || !literal(expression.right, true)) fail('metadata initializer');
    const keys = expression.right.properties.map(property => propertyName(property.name));
    if (new Set(keys).size !== keys.length || keys.some(key => !['props', 'items', 'dynamicProps', 'dynamicItems'].includes(key))) fail('metadata keys');
    owner.statements.push(statement);
  }
  if (!nativeExports.size) fail('no native exports');
  const allowedGlobals = new Set(['Object', 'RegExp', 'Array', 'isNaN', 'undefined', 'require']);
  for (const unit of units) for (const statement of unit.statements) {
    function visit(node) {
      if (node.kind === ts.SyntaxKind.ImportKeyword || node.kind === ts.SyntaxKind.AwaitExpression) fail('dynamic/async code');
      if (ts.isIdentifier(node) && !namePosition(node)) {
        const symbol = referenceSymbol(node);
        const target = symbols.get(symbol);
        if (target && target !== unit) unit.dependencies.add(target.id);
        if ((!symbol || !symbol.declarations?.some(declaration => declaration.getSourceFile() === ast)) && !allowedGlobals.has(node.text)) fail(`unknown global ${node.text}`);
        if (node.text === 'require' && unit.kind !== 'constant') fail('require inside native function');
      }
      ts.forEachChild(node, visit);
    }
    visit(statement);
  }
  const reachable = new Set();
  function reach(id) { if (reachable.has(id)) return; reachable.add(id); for (const dependency of units[id].dependencies) reach(dependency); }
  for (const target of nativeExports.values()) reach(target.id);
  for (const unit of units) if (unit.kind === 'function' && !reachable.has(unit.id)) fail(`unreachable native function ${unit.name}`);
  const byPointer = new Map();
  for (const unit of units) if (reachable.has(unit.id) && unit.kind === 'function') {
    const group = byPointer.get(unit.pointer) ?? []; group.push(unit); byPointer.set(unit.pointer, group);
  }
  // Native AJV can emit multiple independently mutable .errors instances for
  // one schema. Retain every instance and co-locate variants; never deduplicate.
  for (const group of byPointer.values()) for (const left of group) for (const right of group) {
    if (left !== right) left.dependencies.add(right.id);
  }
  const roots = new Set(nativeExports.values());
  const stable = new Map();
  let schemaSequence = 0;
  for (const group of byPointer.values()) {
    group.sort((a, b) => Number(roots.has(b)) - Number(roots.has(a)) || a.id - b.id);
    group.forEach((unit, index) => stable.set(unit.id, `validate_${schemaSequence}_${index}`));
    schemaSequence++;
  }
  function transformed(node, namesById) {
    const result = ts.transform(node, [context => root => {
      const visit = current => {
        if (ts.isShorthandPropertyAssignment(current)) {
          const unit = symbols.get(checker.getShorthandAssignmentValueSymbol(current));
          if (unit) {
            if (current.objectAssignmentInitializer) fail('shorthand initializer');
            return ts.factory.createPropertyAssignment(current.name.text, ts.factory.createIdentifier(namesById.get(unit.id)));
          }
        }
        if (ts.isIdentifier(current) && !namePosition(current)) {
          const unit = symbols.get(referenceSymbol(current));
          if (unit) {
            const replacement = namesById.get(unit.id);
            if (!replacement) fail(`unresolved initializer dependency ${unit.name}`);
            return ts.factory.createIdentifier(replacement);
          }
        }
        return ts.visitEachChild(current, visit, context);
      };
      return ts.visitNode(root, visit);
    }]);
    const text = printer.printNode(ts.EmitHint.Unspecified, result.transformed[0], ast);
    result.dispose(); return text;
  }
  let constantSequence = 0;
  for (const unit of units) if (reachable.has(unit.id) && unit.kind === 'constant') {
    stable.set(unit.id, `value_${constantSequence++}`);
  }
  // Symbol-aware replacement alone is insufficient: introducing a stable name
  // can capture an existing local/parameter/catch binding at a reference site.
  // Native AJV uses counter names; reject any future emitter shape that would
  // require alpha-renaming its locals rather than silently changing behavior.
  const introducedNames = new Set(stable.values());
  function auditCapture(node) {
    if (ts.isIdentifier(node) && !namePosition(node) && introducedNames.has(node.text)
      && !symbols.has(referenceSymbol(node))) fail(`stable name captures lexical binding ${node.text}`);
    ts.forEachChild(node, auditCapture);
  }
  auditCapture(ast);
  // Tarjan SCC includes wrappers, function references and metadata. Imports
  // always form a DAG; function hoisting preserves wrapper initialization.
  let next = 0; const stack = []; const components = [];
  function connect(unit) {
    unit.index = unit.low = next++; stack.push(unit); unit.stacked = true;
    for (const id of unit.dependencies) {
      const dependency = units[id];
      if (dependency.index === undefined) { connect(dependency); unit.low = Math.min(unit.low, dependency.low); }
      else if (dependency.stacked) unit.low = Math.min(unit.low, dependency.index);
    }
    if (unit.low === unit.index) {
      const component = []; let member;
      do { member = stack.pop(); member.stacked = false; component.push(member); } while (member !== unit);
      components.push(component);
    }
  }
  for (const unit of units) if (reachable.has(unit.id) && unit.index === undefined) connect(unit);
  const owner = new Map(); components.forEach((group, index) => group.forEach(unit => owner.set(unit.id, index)));
  let changed = true;
  while (changed) {
    changed = false; const consumers = new Map();
    for (const unit of units) if (reachable.has(unit.id)) for (const id of unit.dependencies) {
      const from = owner.get(unit.id); const to = owner.get(id);
      if (from !== to) { const set = consumers.get(to) ?? new Set(); set.add(from); consumers.set(to, set); }
    }
    for (let index = 0; index < components.length; index++) {
      const group = components[index]; const uses = consumers.get(index);
      if (!group.length || group.some(unit => unit.kind === 'function') || uses?.size !== 1) continue;
      const target = [...uses][0];
      group.forEach(unit => { owner.set(unit.id, target); components[target].push(unit); });
      components[index] = []; changed = true;
    }
  }
  const paths = new Map(); const records = new Map();
  components.forEach((group, index) => {
    if (!group.length) return;
    const pointers = [...new Set(group.filter(unit => unit.pointer).map(unit => unit.pointer))].sort();
    const path = `modules/${pointers.length ? 'schema' : 'shared'}-${index}.cjs`;
    paths.set(index, path); records.set(path, { schemas: pointers, bindings: group.map(unit => ({ name: stable.get(unit.id), kind: unit.kind, ...(unit.pointer ? { schema: unit.pointer } : {}) })).sort((a, b) => a.name.localeCompare(b.name)) });
  });
  const modules = new Map();
  for (let index = 0; index < components.length; index++) {
    const group = components[index]; if (!group.length) continue;
    const imports = new Map();
    for (const unit of group) for (const id of unit.dependencies) {
      const dependencyOwner = owner.get(id); if (dependencyOwner === index) continue;
      const names = imports.get(dependencyOwner) ?? new Set(); names.add(stable.get(id)); imports.set(dependencyOwner, names);
    }
    let code = '"use strict";\n';
    for (const [dependency, names] of [...imports].sort((a, b) => paths.get(a[0]).localeCompare(paths.get(b[0])))) {
      code += `const { ${[...names].sort().join(', ')} } = require(${JSON.stringify('./' + paths.get(dependency).slice('modules/'.length))});\n`;
    }
    for (const statement of group.flatMap(unit => unit.statements).sort((a, b) => a.pos - b.pos)) code += transformed(statement, stable) + '\n';
    for (const unit of [...group].sort((a, b) => stable.get(a.id).localeCompare(stable.get(b.id)))) code += `exports.${stable.get(unit.id)} = ${stable.get(unit.id)};\n`;
    modules.set(paths.get(index), code);
    records.get(paths.get(index)).dependencies = [...imports.keys()].map(key => paths.get(key)).sort();
  }
  const exports = new Map([...nativeExports].map(([name, unit]) => [name, { path: paths.get(owner.get(unit.id)), name: stable.get(unit.id), schema: unit.pointer }]));
  return { modules, exports, provenance: records };
}
