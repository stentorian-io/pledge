const { execFileSync } = require("node:child_process");
const { smokeImage } = require("./publish-release.cjs");

const [image, version] = process.argv.slice(2);
if (!image || !version) throw new Error("Expected a published image and version");
execFileSync("docker", ["pull", "--platform", "linux/amd64", image], { stdio: "inherit" });
smokeImage(image, version);
