describe("desktop shell", () => {
  it("opens the launch screen", async () => {
    const chooseRepo = await $("[data-testid='pick-repo']");
    await chooseRepo.waitForDisplayed({ timeout: 20000 });

    const commandPalette = await $("[data-testid='command-palette']");
    await commandPalette.waitForDisplayed({ timeout: 20000 });
  });
});
