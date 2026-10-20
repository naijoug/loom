import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
const root = process.cwd();
const reachable = new Set();
const visit = file => {
  file = path.resolve(file);
  if (reachable.has(file) || !fs.existsSync(file)) return;
  reachable.add(file);
  if (!/\.[cm]?[jt]sx?$/.test(file)) return;
  const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const read = spec => {
    if (!spec?.startsWith('.')) return;
    const resolved = ts.resolveModuleName(spec, file, { moduleResolution: ts.ModuleResolutionKind.Bundler, resolveJsonModule: true }, ts.sys).resolvedModule?.resolvedFileName;
    if (resolved) visit(resolved);
    else { const asset = path.resolve(path.dirname(file), spec); if (fs.existsSync(asset)) visit(asset); }
  };
  const walk = node => {
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) read(node.moduleSpecifier.text);
    if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword && ts.isStringLiteral(node.arguments[0])) read(node.arguments[0].text);
    if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument) && ts.isStringLiteral(node.argument.literal)) read(node.argument.literal.text);
    ts.forEachChild(node, walk);
  }; walk(source);
};
visit(path.join(root, 'src/main.tsx'));
const all = fs.readdirSync(path.join(root, 'src'), { recursive: true }).map(file => path.join(root, 'src', file)).filter(file => fs.statSync(file).isFile());
const report = { entry: 'src/main.tsx', reachable: [...reachable].map(file => path.relative(root, file)).sort(), unreachable: all.filter(file => !reachable.has(file)).map(file => path.relative(root, file)).sort(), rustEntry: 'src-tauri/src/lib.rs' };
fs.mkdirSync('docs/reviews/simple-chat', { recursive: true });
fs.writeFileSync('docs/reviews/simple-chat/reachability.json', JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify({ reachable: report.reachable.length, unreachable: report.unreachable.filter(file => !file.startsWith('src/workbench/')).length, frozen: report.unreachable.filter(file => file.startsWith('src/workbench/')).length }));

const rustReachable = new Set();
function visitRust(file) {
  file = path.resolve(file);
  if (rustReachable.has(file) || !fs.existsSync(file)) return;
  rustReachable.add(file);
  const name = path.basename(file, '.rs');
  const directory = ['lib', 'main', 'mod'].includes(name) ? path.dirname(file) : path.join(path.dirname(file), name);
  for (const match of fs.readFileSync(file, 'utf8').matchAll(/(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;/g)) {
    const candidates = [path.join(directory, `${match[1]}.rs`), path.join(directory, match[1], 'mod.rs')];
    const child = candidates.find(candidate => fs.existsSync(candidate));
    if (child) visitRust(child);
  }
}
visitRust('src-tauri/src/lib.rs');
report.rustReachable = [...rustReachable].map(file => path.relative(root, file)).sort();
report.rustFrozen = fs.readdirSync('src-tauri/src/workbench', {recursive:true}).filter(file => file.endsWith('.rs')).map(file => `src-tauri/src/workbench/${file}`);
fs.writeFileSync('docs/reviews/simple-chat/reachability.json', JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify({ rustReachable: report.rustReachable.length, rustFrozen: report.rustFrozen.length }));
