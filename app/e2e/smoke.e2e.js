import path from "node:path";
import { fileURLToPath } from "node:url";

const appDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const fixtureRoot = path.join(appDir, ".e2e-tmp");

describe("Git commit workflow", () => {
  it("initializes a repository, stages a file, commits it, and shows history", async () => {
    const runId = `commit-loop-${process.pid}-${Date.now()}`;
    const repoPath = await browser.tauri.execute(
      ({ core }, rootPath, fixtureRunId) =>
        core.invoke("e2e_prepare_repo", { rootPath, runId: fixtureRunId }),
      fixtureRoot,
      runId,
    );

    await browser.execute((repo) => {
      localStorage.setItem("repo.last", repo);
    }, repoPath);
    await browser.refresh();

    const resume = await $("[data-testid='resume-repo']");
    await resume.waitForClickable({ timeout: 20_000 });
    await resume.click();

    const repoShell = await $("[data-testid='repo-shell']");
    await repoShell.waitForDisplayed({ timeout: 20_000 });

    await browser.tauri.execute(
      ({ core }, repo, relativePath, contents) =>
        core.invoke("e2e_write_file", { repoPath: repo, relativePath, contents }),
      repoPath,
      "hello.txt",
      "hello from the desktop E2E workflow\n",
    );

    const unstaged = await $(
      "[data-testid='unstaged-file'][data-file-path='hello.txt']",
    );
    await unstaged.waitForDisplayed({ timeout: 20_000 });
    const stageAction = await $(
      "[data-testid='unstaged-file'][data-file-path='hello.txt'] [data-testid='stage-action']",
    );
    await stageAction.waitForClickable({ timeout: 20_000 });
    await stageAction.click();

    const staged = await $(
      "[data-testid='staged-file'][data-file-path='hello.txt']",
    );
    await staged.waitForDisplayed({ timeout: 20_000 });

    const message = await $("[data-testid='commit-message']");
    await message.setValue("e2e initial commit");
    const commit = await $("[data-testid='commit-action']");
    await commit.waitForEnabled({ timeout: 20_000 });
    await commit.click();
    await staged.waitForExist({ reverse: true, timeout: 20_000 });

    await $("[data-testid='nav-history']").click();
    const subject = await $("[data-testid='commit-subject']");
    await subject.waitForDisplayed({ timeout: 20_000 });
    await expect(subject).toHaveText("e2e initial commit");
  });
});
