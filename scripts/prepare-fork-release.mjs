import fs from "node:fs";

const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(process.env.UPSTREAM_TAG);
if (!match) throw new Error("Expected a stable upstream release tag.");
const { GITHUB_RUN_NUMBER: run, GITHUB_RUN_ATTEMPT: attempt } = process.env;
if (!/^\d+$/.test(run) || !/^\d+$/.test(attempt)) throw new Error("Missing GitHub run number.");
const version = `${match[1]}.${match[2]}.${Number(match[3]) + 1}-fork.${run}.${attempt}`;

for (const file of ["package.json", "src-tauri/tauri.conf.json"]) {
  const value = JSON.parse(fs.readFileSync(file, "utf8"));
  value.version = version;
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}
const cargo = "src-tauri/Cargo.toml";
fs.writeFileSync(cargo, fs.readFileSync(cargo, "utf8").replace(/^version = ".*"$/m, `version = "${version}"`));

const file = "src/content/releases.ts";
const release = {
  version,
  date: new Date().toISOString().slice(0, 10),
  title: "Humla for your Mac",
  summary: "Russian transcription, local AI and Zoom meeting reminders.",
  visual: "updates",
  paragraphs: ["This version includes the latest stable Humla release, GigaAM v3 E2E-RNN-T for Russian, local model thinking controls and reminders to record Zoom meetings."],
  highlights: ["Russian transcription with punctuation on the Apple GPU.", "Thinking controls for local summaries and chat.", "Zoom meeting reminders and updates from your fork."],
  issues: [],
};
const source = fs.readFileSync(file, "utf8");
const marker = "export const releases: Release[] = [";
if (!source.includes(marker)) throw new Error("Release notes format changed.");
fs.writeFileSync(file, source.replace(marker, `${marker}\n${JSON.stringify(release, null, 2)},`));
fs.appendFileSync(process.env.GITHUB_ENV, `RELEASE_VERSION=${version}\n`);
