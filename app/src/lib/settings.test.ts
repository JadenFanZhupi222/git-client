import { describe, expect, it } from "vitest";
import { settingsSectionForEntryPoint } from "./settings";

describe("settings entry-point routing", () => {
  it.each([
    ["githubCommand", "github"],
    ["githubPrPanel", "github"],
    ["githubCreatePrDialog", "github"],
    ["gitlabCommand", "gitlab"],
    ["gitlabMrPanel", "gitlab"],
    ["gitlabCreateMrDialog", "gitlab"],
    ["moreMenu", "deepseek"],
  ] as const)("routes %s to the %s settings section", (source, section) => {
    expect(settingsSectionForEntryPoint(source)).toBe(section);
  });
});
