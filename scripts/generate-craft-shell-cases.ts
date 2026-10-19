/** Regenerate compatibility expectations from the fixed, local Craft source.
 * This evaluates the parser/policy only; none of the fixture commands execute.
 * Run: bun scripts/generate-craft-shell-cases.ts
 */
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
const root=resolve(import.meta.dir,'..');
const repo=resolve(root,'.ref/repos/craft-agents-oss');
const sha='73bd9c2a3573158bea880984eb8d5fdb41e0cac2';
if(execFileSync('git',['-C',repo,'rev-parse','HEAD'],{encoding:'utf8'}).trim()!==sha)throw new Error('Reference checkout differs from v0.14.0');
const source='packages/shared/src/agent/bash-validator.ts';
const upstream=await import(pathToFileURL(resolve(repo,source)).href);
const defaults=JSON.parse(readFileSync(resolve(repo,'apps/electron/resources/permissions/default.json'),'utf8'));
const patterns=defaults.allowedBashPatterns.filter((p:any)=>!p.pattern.includes('craft-agent')).map((p:any)=>({...p,regex:new RegExp(p.pattern),source:p.pattern}));
const commands=[
 'ls #comment\\\nrm forbidden', 'echo a\\\n#word', 'ls \\#literal', 'ls \\#literal; rm forbidden',
 '', ' ', '# comment', 'ls', 'ls -la', 'ls;','ls #comment','ls a* ? [ab]','ls /tmp/中文','ls "two words"',
 'git status', 'git diff --stat', 'git log -5', 'git checkout main', 'git push', 'printf test', 'echo test', 'echo "hello world"',
 'cat file', 'cat "a b"', "cat 'a b'", 'cat a\\ b', 'cat -- ./file', 'cat ./--file',
 'git status && git log', 'git status || git log', 'git status; git log', 'git status\ngit log',
 '(git status; cat file) | head', '(ls && (pwd))', 'ls &', 'ls & pwd', 'ls |& head',
 'ls && touch forbidden', 'ls | touch forbidden', 'ls; rm forbidden', 'echo x > forbidden',
 '>/dev/null', 'cat &>/dev/null', 'cat &>forbidden', 'cat 0<&0',
 'cat < input', 'cat > /dev/null', 'cat 2>&1', 'cat >> /dev/null', 'cat <> file', 'cat <<EOF\nhello\nEOF',
 'cat <<<hello','>file', '2>file ls', 'ls 2>/dev/null', 'cat <(touch forbidden)', 'echo $(touch forbidden)',
 'echo `touch forbidden`','cat "$HOME/file"','cat ${HOME}/file', "echo '$(touch forbidden)'", "echo '$HOME'",
 'echo "$(touch forbidden)"','echo "a\\q"','echo a\\\nb','FOO=x ls','PATH=/evil ls','env ls','command ls','/bin/ls',
 'if true; then ls; fi','for x in a; do ls; done','while true; do ls; done','f(){ ls; }; f','{ ls; }','! ls',
 "find . -name '*.rs'", 'find . -exec touch f \\;', 'find . -delete', 'find . -ok rm {} \\;',
 "awk '{print $1}' file", "awk 'BEGIN {system(\"touch f\")}'", "awk 'BEGIN {\"cmd\" | getline}'",
 "awk 'BEGIN {print | \"cmd\"}'", 'sort file', 'sort -n file','sort -oout file','sort -ro out file',
 'sort --output=out file','sort --compress-program=cmd file', 'sort -t o file', 'sort -k1,2 file', 'sort -- -output',
 "sed -n '1,5p' file", "sed -n '/foo/p' file", "sed -n 's/foo/bar/p' file", "sed -n 's#foo#bar#gp' file",
 "sed -n 'y/abc/xyz/;p' file", "sed -n ':a;N;$!ba;p' file", "sed -n -e '1p' -e '2p' file",
 "sed -n -e'1p' file", "sed -n 's/x/y/w out' file", "sed -n 's/x/y/e' file", "sed -n 'e cmd' file",
 "sed -n 'w out' file", "sed -n -i '' 'p' file", "sed -ni 'p' file", 'sed -n --file=script file',
 "sed -n 'r file' file", "sed -n 'p' file", "sed -n -- '1p' file", 'sed -n -l 80 p file',
 'gh api repos/owner/repo', 'gh api -XGET repos/owner/repo', 'gh api -X DELETE repos/owner/repo',
 'gh api -iXDELETE repos/owner/repo','gh api --method=POST repos/owner/repo', 'gh api -f key=value repos/owner/repo',
 'gh api -X GET -f key=value repos/owner/repo', 'gh api --input body repos/owner/repo',
 "gh api -H 'X-HTTP-Method-Override: DELETE' repos/owner/repo", "gh api graphql -f 'query={ viewer { login } }'",
 "gh api graphql -f 'query=mutation { deleteThing }'", 'gh api graphql -F query=@file', 'gh api graphql --input file',
 'npm --version', 'python -m json.tool file', 'jq . file', 'rg needle .', 'curl https://example.invalid',
 'ls "unterminated', 'echo "a\'$(touch f)\'"', 'echo ""', "echo ''", 'echo "a"\'b\'c',
];
const corrections=[
 {command:'cat 0<&$FD',reason:'Redirect expansions must not inherit an unchecked environment-dependent target'},
 {command:'cat <$(touch forbidden)',reason:'Executable redirect expansion must be rejected before shell execution'},
 {command:'cat <`touch forbidden`',reason:'Executable redirect expansion must be rejected before shell execution'},
 {command:'find . -fprint forbidden',reason:'find file-output flags write files'},
 {command:"awk 'BEGIN {print 1 > \"forbidden\"}'",reason:'awk output redirects write files'},
 {command:"rg --pre 'touch forbidden' needle file",reason:'rg preprocessing executes a program'},
];
const cases=[...commands.map(command=>({command})), ...corrections.map(c=>({...c,loomAllowed:false}))].map((item:any)=>{
 let upstreamAllowed=false;let upstreamError:string|undefined;
 try{upstreamAllowed=upstream.validateBashCommand(item.command,patterns).allowed;}catch{upstreamError='parser threw';}
 const plainCorrection=item.command==='echo a\\\n#word'?{loomPlain:['echo','a#word'],plainCorrection:'Bash continuation preserves # inside an existing word; verified with the Bash builtin printf'}:{};
 return {...item,...plainCorrection,upstreamAllowed,...(upstreamError?{upstreamError}:{}),upstreamPlain:upstream.parseSimpleCommand(item.command)};
});
const output=resolve(root,'crates/loom-runtime/tests/fixtures/craft-shell-cases.json');mkdirSync(resolve(output,'..'),{recursive:true});
writeFileSync(output,JSON.stringify({upstreamSha:sha,upstreamSource:source,upstreamSourceSha256:createHash('sha256').update(readFileSync(resolve(repo,source))).digest('hex'),defaultRulesSha256:createHash('sha256').update(readFileSync(resolve(repo,'apps/electron/resources/permissions/default.json'))).digest('hex'),cases},null,2)+'\n');
console.log(`Generated ${cases.length} actual upstream classifications; commands were not executed.`);
