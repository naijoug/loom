/** Fixed upstream name classification only; no tools or network are executed. */
import { readFileSync,writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
const root=resolve(import.meta.dir,'..');const repo=resolve(root,'.ref/repos/craft-agents-oss');const sha='73bd9c2a3573158bea880984eb8d5fdb41e0cac2';
if(execFileSync('git',['-C',repo,'rev-parse','HEAD'],{encoding:'utf8'}).trim()!==sha)throw new Error('Unexpected upstream revision');
const file=resolve(repo,'packages/shared/src/agent/mcp-tool-names.ts');
const {isReadOnlyMcpToolName}=await import(pathToFileURL(file).href);
const verbs=JSON.parse(readFileSync(resolve(repo,'apps/electron/resources/permissions/default.json'),'utf8')).allowedMcpPatterns.map((p:any)=>p.pattern);
const actions=['get_issue','listIssues','getOrCreateIssue','get_or_create_issue','search_and_replace','run_query','mark_as_read','delete_account','send_thread_reply','post_status','upload_info','status','count_documents','info','exists','query','rebuild_search_index','refresh_materialized_view','get_commit','get_post','get_block','get_charge','list_trash','issue_read','notion-search','slack_get_channel_history','alpha_get_record','getURLInfo','GET_RECORD','readFile','find_file','fetch_data','inspect','view_page','set_status','get_update','delete_read','search','other','a_count','a_status','resource_info','readOnly','get1Thing'];
const cases=['alpha','budget','slack'].flatMap(source=>actions.map(action=>{const name=`mcp__${source}__${action}`;return {source,name,allowed:isReadOnlyMcpToolName(name,verbs)};}));
writeFileSync(resolve(root,'crates/loom-runtime/tests/fixtures/craft-mcp-cases.json'),JSON.stringify({upstreamSha:sha,sourceSha256:createHash('sha256').update(readFileSync(file)).digest('hex'),cases},null,2)+'\n');
console.log(`Generated ${cases.length} executed upstream MCP classifications.`);
