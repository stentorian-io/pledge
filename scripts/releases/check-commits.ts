import { readFileSync } from "node:fs";
import { validateMessage } from "../lib/commit-message.cjs";
import { fail, output } from "../lib/command";

const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH!, "utf8"));
if (event.pull_request) {
  const error = validateMessage(`${event.pull_request.title}\n\n${event.pull_request.body ?? ""}`);
  if (error) fail("pull-request", error);
} else if (event.before && !/^0+$/.test(event.before)) {
  const commits = output("git", ["rev-list", `${event.before}..${event.after}`]).trim().split("\n").filter(Boolean);
  for (const commit of commits) {
    const message = output("git", ["show", "-s", "--format=%B", commit]);
    const error = validateMessage(message);
    if (error) fail("commit", `${commit}: ${error}`);
  }
}
