import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';

function inspect(name: string, source: string) {
  const file = ts.createSourceFile(name, source, ts.ScriptTarget.Latest, true,
    name.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const findings: string[] = [];
  function report(node: ts.Node, reason: string) {
    const { line } = file.getLineAndCharacterOfPosition(node.getStart(file));
    findings.push(`${name}:${line + 1}: ${reason}`);
  }
  function inJsx(node: ts.Node): boolean {
    for (let parent = node.parent; parent; parent = parent.parent) {
      if (!ts.isJsxExpression(parent)) continue;
      return !(ts.isJsxAttribute(parent.parent) && /^on[A-Z]/.test(parent.parent.name.getText(file)));
    }
    return false;
  }
  function failureMessage(node: ts.Node): boolean {
    for (let current = node; current.parent; current = current.parent) {
      if (ts.isNewExpression(current.parent)) {
        return current.parent.expression.getText(file) === 'ApiFailure' && current.parent.arguments?.[1] === current;
      }
    }
    return false;
  }
  function visit(node: ts.Node) {
    if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression)
      && node.expression.expression.getText(file) === 'JSON'
      && node.expression.name.text === 'stringify' && inJsx(node)) {
      report(node, 'serialized data must use a readable details view');
    }
    if ((ts.isStringLiteralLike(node) || ts.isJsxText(node) || ts.isTemplateHead(node)
      || ts.isTemplateMiddle(node) || ts.isTemplateTail(node)) && /\bjson\b/i.test(node.text)) {
      if (ts.isJsxText(node) || ts.isJsxAttribute(node.parent) || inJsx(node) || failureMessage(node)) {
        report(node, 'interface wording must describe the data or recovery action');
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(file);
  return findings;
}

function sourceFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return entry.name === 'generated' ? [] : sourceFiles(path);
    return /\.tsx?$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name) ? [path] : [];
  });
}

describe('readable interface boundaries', () => {
  it('detects serialized JSX and technical response wording while permitting transport serialization', () => {
    expect(inspect('view.tsx', 'const View = () => <pre>{JSON.stringify(value)}</pre>;')).toHaveLength(1);
    expect(inspect('view.tsx', 'const View = () => <div title="JSON details">JSON</div>;')).toHaveLength(2);
    expect(inspect('api.ts', "throw new ApiFailure('INVALID', 'JSON response invalid');")).toHaveLength(1);
    expect(inspect('api.ts', "throw new ApiFailure('INVALID', `JSON response ${status}`);")).toHaveLength(1);
    expect(inspect('view.tsx', 'const body = JSON.stringify(value); const View = () => <button onClick={() => send(JSON.stringify(value))}>发送</button>;')).toEqual([]);
  });

  it('keeps serialized object dumps and JSON wording out of product renderers and errors', () => {
    const root = fileURLToPath(new URL('.', import.meta.url));
    const findings = sourceFiles(root).flatMap(path => inspect(path, readFileSync(path, 'utf8')));
    expect(findings).toEqual([]);
  });
});
