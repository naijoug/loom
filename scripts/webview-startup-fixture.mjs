#!/usr/bin/env node
// Produces test-only frontend resources. No fault switch is included in production code.
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root=fileURLToPath(new URL('..',import.meta.url));
const mode=process.argv[2];
if(!['once','always'].includes(mode)){console.error('Usage: node scripts/webview-startup-fixture.mjs once|always (run pnpm build first)');process.exit(2);}
const dist=join(root,'dist'),base=join(root,'.tmp/webview-fixtures'),target=join(base,mode);
if(!existsSync(join(dist,'index.html')))throw Error('Run pnpm build first');
const html=readFileSync(join(dist,'index.html'),'utf8');
const entry=html.match(/<script type="module" crossorigin src="([^"]+)"><\/script>/);
if(!entry)throw Error('Expected exactly one production module entry');
mkdirSync(base,{recursive:true});rmSync(target,{recursive:true,force:true});cpSync(dist,target,{recursive:true});
writeFileSync(join(target,'index.html'),html.replace(entry[0],'<script type="module" src="/startup-fixture.js"></script>'));
writeFileSync(join(target,'startup-fixture.js'),mode==='once'
  ? `if (sessionStorage.getItem('loom-startup-fixture-once')) { import(${JSON.stringify(entry[1])}); } else { sessionStorage.setItem('loom-startup-fixture-once','1'); }\n`
  : '// Deliberately never load the app. Native startup recovery must stop after one automatic retry.\n');
const config=join(base,`${mode}.json`);
writeFileSync(config,JSON.stringify({build:{beforeBuildCommand:'',frontendDist:resolve(target)}},null,2)+'\n');
console.log(`Test config: ${config}\nBuild: pnpm tauri build --bundles app --config .tmp/webview-fixtures/${mode}.json\nRestore normal bundle afterwards: pnpm tauri build --bundles app`);
