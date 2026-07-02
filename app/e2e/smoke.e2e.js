describe("desktop shell", () => {
  it("opens the launch screen", async () => {
    const chooseRepo = await $("button[aria-label='选择仓库']");
    await chooseRepo.waitForDisplayed();

    const commandPalette = await $("button[aria-label='命令面板']");
    await commandPalette.waitForDisplayed();
  });
});
