import type { BranchDto, RefDto } from "../ipc";

export function localBranchChoices(
  branches: BranchDto[] | undefined,
  currentBranch: string | null,
): string[] {
  return unique([
    ...(currentBranch ? [currentBranch] : []),
    ...(branches ?? []).map((branch) => branch.name),
  ]);
}

export function remoteBranchChoices(
  refs: RefDto[] | undefined,
  preferredRemote: string | null,
): string[] {
  const names = (refs ?? [])
    .filter((ref) => ref.kind === "remote")
    .map((ref) => shortRemoteBranchName(ref.name, preferredRemote))
    .filter(Boolean);
  return unique(names);
}

export function defaultBaseBranch(choices: string[]): string {
  return (
    choices.find((choice) => choice === "main") ??
    choices.find((choice) => choice === "master") ??
    choices.find((choice) => choice === "develop") ??
    choices[0] ??
    "main"
  );
}

function shortRemoteBranchName(
  name: string,
  preferredRemote: string | null,
): string {
  if (preferredRemote && name.startsWith(`${preferredRemote}/`)) {
    return name.slice(preferredRemote.length + 1);
  }
  const slash = name.indexOf("/");
  return slash >= 0 ? name.slice(slash + 1) : name;
}

function unique(values: string[]): string[] {
  return [...new Set(values.map((value) => value.trim()).filter(Boolean))];
}
