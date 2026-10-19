import { readFileSync, writeFileSync, mkdirSync, readdirSync, copyFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { validateVersion } from "./prepare-app-release.mjs";

export function manifest(version, repository, assets, notes = "") {
  validateVersion(`v${version}`,[version]);
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository??"")) throw new Error("Invalid release repository");
  const platforms={};
  for(const arch of ["aarch64","x86_64"]){
    const name=`Loom_${version}_${arch}.app.tar.gz`;
    const signature=assets[`${name}.sig`]?.trim();
    if(!assets[name]||!signature)throw new Error(`Missing package/signature for ${arch}`);
    // Signature contains signed bytes, not a path/URL.
    if(!Buffer.from(signature,"base64").toString("utf8").startsWith("untrusted comment:"))throw new Error(`Malformed signature for ${arch}`);
    platforms[`darwin-${arch}`]={url:`https://github.com/${repository}/releases/download/v${version}/${name}`,signature};
  }
  return {version,notes,pub_date:new Date().toISOString(),platforms};
}
export function main(args=process.argv.slice(2)){
  const [mode,directory,target]=args;
  const version=validateVersion(process.env.RELEASE_TAG,[JSON.parse(readFileSync("src-tauri/tauri.conf.json","utf8")).version]);
  if(mode==="stage"){
    const arch=target?.split("-")[0];
    if(!["aarch64","x86_64"].includes(arch))throw new Error("Unsupported architecture");
    const bundle=resolve(`src-tauri/target/${target}/release/bundle`);
    mkdirSync(directory,{recursive:true});
    const base=`Loom_${version}_${arch}`;
    copyFileSync(join(bundle,"macos/Loom.app.tar.gz"),join(directory,`${base}.app.tar.gz`));
    copyFileSync(join(bundle,"macos/Loom.app.tar.gz.sig"),join(directory,`${base}.app.tar.gz.sig`));
    const dmgs=readdirSync(join(bundle,"dmg")).filter(name=>name.endsWith(".dmg"));
    if(dmgs.length!==1)throw new Error("Expected exactly one DMG");
    copyFileSync(join(bundle,"dmg",dmgs[0]),join(directory,`${base}.dmg`));
  }else if(mode==="manifest"){
    const assets={};
    for(const name of readdirSync(directory))assets[name]=name.endsWith(".sig")?readFileSync(join(directory,name),"utf8"):true;
    for(const arch of ["aarch64","x86_64"])if(!assets[`Loom_${version}_${arch}.dmg`])throw new Error(`Missing ${arch} DMG`);
    const output=manifest(version,process.env.LOOM_RELEASE_REPOSITORY,assets,process.env.RELEASE_NOTES??`Loom ${version}`);
    writeFileSync(join(directory,"latest.json"),JSON.stringify(output,null,2));
    const checksums=readdirSync(directory).filter(name=>name!=="SHA256SUMS").sort().map(name=>`${createHash("sha256").update(readFileSync(join(directory,name))).digest("hex")}  ${name}`).join("\n");
    writeFileSync(join(directory,"SHA256SUMS"),checksums+"\n");
  }else throw new Error("Expected stage <directory> <target> or manifest <directory>");
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))main();
