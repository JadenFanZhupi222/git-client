describe("Git commit workflow", () => {
  it("initializes a repository, stages a file, commits it, and shows history", async () => {
    const runId = `commit-loop-${process.pid}-${Date.now()}`;
    const repoPath = await browser.tauri.execute(
      ({ core }, fixtureRunId) =>
        core.invoke("e2e_prepare_repo", { runId: fixtureRunId }),
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
      ({ core }, fixtureRunId, relativePath, contents) =>
        core.invoke("e2e_write_file", { runId: fixtureRunId, relativePath, contents }),
      runId,
      "hello.txt",
      "hello from the desktop E2E workflow\n",
    );

    const refresh = await $("[data-testid='refresh-status']");
    await refresh.waitForEnabled({ timeout: 20_000 });
    await refresh.click();

    const unstaged = await $(
      "[data-testid='unstaged-file'][data-file-path='hello.txt']",
    );
    await unstaged.waitForDisplayed({ timeout: 20_000 });
    const stageAction = await $(
      "[data-testid='unstaged-file'][data-file-path='hello.txt'] [data-testid='stage-action']",
    );
    await stageAction.waitForClickable({ timeout: 20_000 });
    await browser.execute(() => {
      const action = document.querySelector(
        "[data-testid='unstaged-file'][data-file-path='hello.txt'] [data-testid='stage-action']",
      );
      if (!(action instanceof HTMLButtonElement)) {
        throw new Error("Stage action button is not available");
      }
      action.click();
    });

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
