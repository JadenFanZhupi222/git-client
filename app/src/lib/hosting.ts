export type HostingProvider = "github" | "gitlab" | "bitbucket";

export interface HostingRemote {
  provider: HostingProvider;
  owner: string;
  repo: string;
  webBaseUrl: string;
}

export interface RemoteLike {
  name: string;
  url: string;
}

export interface ChangeRequestLink {
  provider: HostingProvider;
  remoteName: string;
  url: string;
}

const SUPPORTED_HOSTS: Record<string, HostingProvider> = {
  "github.com": "github",
  "gitlab.com": "gitlab",
  "bitbucket.org": "bitbucket",
};

export function detectHostingRemote(remoteUrl: string): HostingRemote | null {
  const parsed = parseRemoteUrl(remoteUrl.trim());
  if (!parsed) return null;

  const provider = SUPPORTED_HOSTS[parsed.host.toLowerCase()];
  if (!provider) return null;

  const parts = parsed.path
    .replace(/^\/+/, "")
    .replace(/\.git$/i, "")
    .split("/")
    .filter(Boolean);
  if (parts.length < 2) return null;

  const repo = parts[parts.length - 1];
  const owner = parts.slice(0, -1).join("/");
  const webBaseUrl = `https://${parsed.host}/${owner}/${repo}`;
  return { provider, owner, repo, webBaseUrl };
}

export function buildCreateChangeRequestUrl(
  remotes: RemoteLike[],
  branch: string | null,
  preferredRemote: string | null,
): ChangeRequestLink | null {
  if (!branch) return null;

  for (const remote of orderedRemotes(remotes, preferredRemote)) {
    const hosting = detectHostingRemote(remote.url);
    if (!hosting) continue;
    return {
      provider: hosting.provider,
      remoteName: remote.name,
      url: createUrl(hosting, branch),
    };
  }

  return null;
}

export function buildFindChangeRequestUrl(
  remotes: RemoteLike[],
  branch: string | null,
  preferredRemote: string | null,
): ChangeRequestLink | null {
  if (!branch) return null;

  for (const remote of orderedRemotes(remotes, preferredRemote)) {
    const hosting = detectHostingRemote(remote.url);
    if (!hosting || hosting.provider !== "github") continue;
    return {
      provider: hosting.provider,
      remoteName: remote.name,
      url: `${hosting.webBaseUrl}/pulls?${githubPullSearchQuery(branch)}`,
    };
  }

  return null;
}

function parseRemoteUrl(
  remoteUrl: string,
): { host: string; path: string } | null {
  if (!remoteUrl) return null;

  try {
    const url = new URL(remoteUrl);
    if (
      url.protocol === "http:" ||
      url.protocol === "https:" ||
      url.protocol === "ssh:"
    ) {
      return { host: url.hostname, path: url.pathname };
    }
  } catch {
    // Fall through to scp-style SSH remotes such as git@github.com:owner/repo.git.
  }

  const ssh = remoteUrl.match(/^(?:[^@]+@)?([^:]+):(.+)$/);
  if (!ssh) return null;
  return { host: ssh[1], path: ssh[2] };
}

function createUrl(remote: HostingRemote, branch: string): string {
  const source = encodeURIComponent(branch);
  switch (remote.provider) {
    case "github":
      return `${remote.webBaseUrl}/compare/${source}?expand=1`;
    case "gitlab":
      return `${remote.webBaseUrl}/-/merge_requests/new?merge_request%5Bsource_branch%5D=${source}`;
    case "bitbucket":
      return `${remote.webBaseUrl}/pull-requests/new?source=${source}`;
  }
}

function orderedRemotes(
  remotes: RemoteLike[],
  preferredRemote: string | null,
): RemoteLike[] {
  return [
    ...remotes.filter((remote) => remote.name === preferredRemote),
    ...remotes.filter((remote) => remote.name !== preferredRemote),
  ];
}

function githubPullSearchQuery(branch: string): string {
  return new URLSearchParams({
    q: `is:pr is:open head:${branch}`,
  }).toString();
}
