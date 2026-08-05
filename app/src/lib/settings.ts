import type { CredentialKindDto } from "../bindings";

export type SettingsSection = CredentialKindDto;

export type SettingsEntryPoint =
  | "githubCommand"
  | "githubPrPanel"
  | "githubCreatePrDialog"
  | "gitlabCommand"
  | "gitlabMrPanel"
  | "gitlabCreateMrDialog"
  | "moreMenu";

const ENTRY_POINT_SECTIONS: Record<SettingsEntryPoint, SettingsSection> = {
  githubCommand: "github",
  githubPrPanel: "github",
  githubCreatePrDialog: "github",
  gitlabCommand: "gitlab",
  gitlabMrPanel: "gitlab",
  gitlabCreateMrDialog: "gitlab",
  moreMenu: "deepseek",
};

export function settingsSectionForEntryPoint(
  entryPoint: SettingsEntryPoint,
): SettingsSection {
  return ENTRY_POINT_SECTIONS[entryPoint];
}
