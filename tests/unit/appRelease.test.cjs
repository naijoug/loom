const test=require('node:test');const assert=require('node:assert/strict');
test('release refuses mismatched versions and invalid key configuration',async()=>{
 const {validateVersion,releaseConfig}=await import('../../scripts/prepare-app-release.mjs');
 assert.equal(validateVersion('v0.2.0',['0.2.0','0.2.0','0.2.0']),'0.2.0');
 for(const tag of ['main','v01.2.0','v0.2.0-beta.1'])assert.throws(()=>validateVersion(tag,['0.2.0']));
 assert.throws(()=>validateVersion('v0.2.0',['0.1.0']));
 assert.throws(()=>releaseConfig({LOOM_RELEASE_REPOSITORY:'owner/repo',LOOM_UPDATER_PUBLIC_KEY:'/path/key'}));
 const pubkey=Buffer.from('untrusted comment: test\nRWfake').toString('base64');
 const config=releaseConfig({LOOM_RELEASE_REPOSITORY:'owner/releases',LOOM_UPDATER_PUBLIC_KEY:pubkey});
 assert.equal(config.bundle.createUpdaterArtifacts,true);
 assert.deepEqual(config.plugins.updater.endpoints,['https://github.com/owner/releases/releases/latest/download/latest.json']);
});
test('manifest requires both architectures and embeds signature content',async()=>{
 const {manifest}=await import('../../scripts/package-app-release.mjs');
 const assets={};const signature=Buffer.from('untrusted comment: test signature\nRWfake').toString('base64');
 for(const arch of ['aarch64','x86_64']){
  assets[`Loom_0.2.0_${arch}.app.tar.gz`]=true;
  assets[`Loom_0.2.0_${arch}.app.tar.gz.sig`]=signature+'\n';
 }
 const data=manifest('0.2.0','owner/releases',assets,'Release notes');
 assert.deepEqual(Object.keys(data.platforms),['darwin-aarch64','darwin-x86_64']);
 assert.equal(data.platforms['darwin-aarch64'].signature,signature);
 assert.match(data.platforms['darwin-x86_64'].url,/\/v0\.2\.0\/Loom_0\.2\.0_x86_64\.app\.tar\.gz$/);
 delete assets['Loom_0.2.0_x86_64.app.tar.gz.sig'];assert.throws(()=>manifest('0.2.0','owner/releases',assets));
});
