const { execSync } = require("child_process");
const path = require("path");
const fs = require("fs");

const getCommitSha = () => {
  try {
    return execSync("git rev-parse --short HEAD").toString().trim();
  } catch (error) {
    console.warn("Could not get git commit SHA:", error.message);
    return "unknown";
  }
};

const getPackageVersion = () => {
  try {
    const packageJsonPath = path.resolve(__dirname, "..", "package.json");
    const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
    return packageJson.version;
  } catch (error) {
    console.warn("Could not get package version:", error.message);
    return "unknown";
  }
};

const commitSha = getCommitSha();
const appVersion = getPackageVersion();

const command = process.argv[2]; // e.g., 'dev', 'build:production', 'build:staging'
const viteCommand = process.argv.slice(3).join(" "); // Remaining arguments for vite

if (!command) {
  console.error(
    "Usage: node scripts/run-vite-with-env.js <command> [vite_args...]"
  );
  process.exit(1);
}

let fullCommand = "";

switch (command) {
  case "dev":
    fullCommand = `vite ${viteCommand}`;
    break;
  case "build:production":
    fullCommand = `tsc -b && vite build ${viteCommand}`;
    break;
  case "build:staging":
    fullCommand = `tsc -b && vite build --mode staging ${viteCommand}`;
    break;
  default:
    console.error(`Unknown command: ${command}`);
    process.exit(1);
}

console.log(
  `Executing: ${fullCommand} with __APP_VERSION__=${appVersion} and __COMMIT_SHA__=${commitSha}`
);

try {
  execSync(fullCommand, {
    stdio: "inherit",
    env: {
      ...process.env,
      APP_VERSION: appVersion,
      COMMIT_SHA: commitSha,
    },
  });
} catch (error) {
  console.error("Command execution failed:", error.message);
  process.exit(error.status || 1);
}
