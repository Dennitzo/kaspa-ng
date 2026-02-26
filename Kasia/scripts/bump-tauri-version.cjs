const fs = require("fs");
const path = require("path");

const rootDir = path.resolve(__dirname, "..");
const packageJsonPath = path.join(rootDir, "package.json");
const tauriConfJsonPath = path.join(rootDir, "src-tauri", "tauri.conf.json");
const tauriCargoTomlPath = path.join(rootDir, "src-tauri", "Cargo.toml");

try {
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
  const newVersion = packageJson.version;

  // Update tauri.conf.json
  const tauriConfJson = JSON.parse(fs.readFileSync(tauriConfJsonPath, "utf8"));
  if (tauriConfJson.version !== newVersion) {
    tauriConfJson.version = newVersion;
    fs.writeFileSync(
      tauriConfJsonPath,
      JSON.stringify(tauriConfJson, null, 2) + "\n",
      "utf8"
    );
    console.log(`Updated src-tauri/tauri.conf.json to version: ${newVersion}`);
  } else {
    console.log(
      `src-tauri/tauri.conf.json is already at version: ${newVersion}`
    );
  }

  // Update Cargo.toml
  let tauriCargoToml = fs.readFileSync(tauriCargoTomlPath, "utf8");
  const versionRegex = /(version\s*=\s*\")(\d+\.\d+\.\d+)(-?.*?)(\")/;
  if (tauriCargoToml.match(versionRegex)?.[2] !== newVersion) {
    tauriCargoToml = tauriCargoToml.replace(versionRegex, `$1${newVersion}$4`);
    fs.writeFileSync(tauriCargoTomlPath, tauriCargoToml, "utf8");
    console.log(`Updated src-tauri/Cargo.toml to version: ${newVersion}`);
  } else {
    console.log(`src-tauri/Cargo.toml is already at version: ${newVersion}`);
  }
} catch (error) {
  console.error("Error bumping Tauri version:", error);
  process.exit(1);
}
