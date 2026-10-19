import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function validateVersion(tag, versions) {
  if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag ?? "")) throw new Error("Release tag must be stable SemVer: vX.Y.Z");
  const version = tag.slice(1);
  if (versions.some(value => value !== version)) throw new Error("Tag, package.json, Cargo.toml and tauri.conf.json versions must match");
  return version;
}
export function releaseConfig(env) {
  const repository = env.LOOM_RELEASE_REPOSITORY;
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? "")) throw new Error("LOOM_RELEASE_REPOSITORY must be owner/repo");
  const key = env.LOOM_UPDATER_PUBLIC_KEY?.trim();
  const decoded = Buffer.from(key ?? "", "base64").toString("utf8");
  if (!decoded.startsWith("untrusted comment:") || !decoded.split(/\r?\n/)[1]) throw new Error("LOOM_UPDATER_PUBLIC_KEY must contain the Tauri-generated public key, not its path");
  return { bundle: { createUpdaterArtifacts: true }, plugins: { updater: { pubkey: key, endpoints: [`https://github.com/${repository}/releases/latest/download/latest.json`] } } };
}
export function main() {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const config = JSON.parse(readFileSync(resolve(root,"src-tauri/tauri.conf.json"),"utf8"));
  const pkg = JSON.parse(readFileSync(resolve(root,"package.json"),"utf8"));
  const cargo = readFileSync(resolve(root,"src-tauri/Cargo.toml"),"utf8").match(/^version = "([^"]+)"/m)?.[1];
  const version = validateVersion(process.env.RELEASE_TAG,[config.version,pkg.version,cargo]);
  const output = releaseConfig(process.env);
  mkdirSync(resolve(root,".tmp"),{recursive:true});
  writeFileSync(resolve(root,".tmp/tauri.updater.conf.json"),JSON.stringify(output,null,2));
  console.log(`Validated release ${version}; wrote .tmp/tauri.updater.conf.json (public configuration only)`);
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
