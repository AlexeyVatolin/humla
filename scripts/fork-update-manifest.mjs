import fs from "node:fs";

const { RELEASE_VERSION: version, RELEASE_BASE_URL: base } = process.env;
if (!version || !base) throw new Error("Release version and URL are required.");
const bundle = process.env.BUNDLE_DIR;
if (!bundle) throw new Error("Bundle directory is required.");
const payload = `${bundle}/macos/Humla.app.tar.gz`;
const manifest = {
  version,
  notes: `Humla ${version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    "darwin-aarch64": {
      signature: fs.readFileSync(`${payload}.sig`, "utf8").trim(),
      url: `${base}/download/v${version}/Humla.app.tar.gz`,
    },
  },
};
if (!fs.statSync(payload).size) throw new Error("Empty updater payload.");
fs.writeFileSync(`${bundle}/latest.json`, `${JSON.stringify(manifest, null, 2)}\n`);
