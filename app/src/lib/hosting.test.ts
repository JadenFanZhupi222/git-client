import { describe, expect, it } from "vitest";
import {
  buildCreateChangeRequestUrl,
  buildFindChangeRequestUrl,
  detectHostingRemote,
} from "./hosting";

const remote = (name: string, url: string) => ({ name, url });

describe("detectHostingRemote", () => {
  it("detects GitHub HTTPS remotes", () => {
    expect(detectHostingRemote("https://github.com/acme/project.git")).toEqual({
      provider: "github",
      owner: "acme",
      repo: "project",
      webBaseUrl: "https://github.com/acme/project",
    });
  });

  it("detects GitLab SSH remotes", () => {
    expect(
      detectHostingRemote("git@gitlab.com:team/subgroup/repo.git"),
    ).toEqual({
      provider: "gitlab",
      owner: "team/subgroup",
      repo: "repo",
      webBaseUrl: "https://gitlab.com/team/subgroup/repo",
    });
  });

  it("detects Bitbucket HTTPS remotes", () => {
    expect(
      detectHostingRemote("https://bitbucket.org/workspace/repo.git"),
    ).toEqual({
      provider: "bitbucket",
      owner: "workspace",
      repo: "repo",
      webBaseUrl: "https://bitbucket.org/workspace/repo",
    });
  });

  it("returns null for unsupported hosts and malformed values", () => {
    expect(
      detectHostingRemote("https://example.com/acme/project.git"),
    ).toBeNull();
    expect(detectHostingRemote("not a remote")).toBeNull();
  });
});

describe("buildCreateChangeRequestUrl", () => {
  it("builds a GitHub compare URL for the preferred remote", () => {
    expect(
      buildCreateChangeRequestUrl(
        [remote("origin", "https://github.com/acme/project.git")],
        "feature/ui polish",
        "origin",
      ),
    ).toEqual({
      provider: "github",
      remoteName: "origin",
      url: "https://github.com/acme/project/compare/feature%2Fui%20polish?expand=1",
    });
  });

  it("builds a GitLab merge request URL", () => {
    expect(
      buildCreateChangeRequestUrl(
        [remote("origin", "git@gitlab.com:team/subgroup/repo.git")],
        "feature/api",
        null,
      )?.url,
    ).toBe(
      "https://gitlab.com/team/subgroup/repo/-/merge_requests/new?merge_request%5Bsource_branch%5D=feature%2Fapi",
    );
  });

  it("builds a Bitbucket pull request URL", () => {
    expect(
      buildCreateChangeRequestUrl(
        [remote("origin", "https://bitbucket.org/workspace/repo.git")],
        "feature/api",
        null,
      )?.url,
    ).toBe(
      "https://bitbucket.org/workspace/repo/pull-requests/new?source=feature%2Fapi",
    );
  });

  it("falls back to the first supported remote when the preferred remote is unsupported", () => {
    expect(
      buildCreateChangeRequestUrl(
        [
          remote("mirror", "https://example.com/acme/project.git"),
          remote("origin", "https://github.com/acme/project.git"),
        ],
        "feature/api",
        "mirror",
      )?.remoteName,
    ).toBe("origin");
  });

  it("returns null without a supported remote or branch", () => {
    expect(buildCreateChangeRequestUrl([], "feature/api", null)).toBeNull();
    expect(
      buildCreateChangeRequestUrl(
        [remote("origin", "https://github.com/acme/project.git")],
        null,
        "origin",
      ),
    ).toBeNull();
  });
});

describe("buildFindChangeRequestUrl", () => {
  it("builds a GitHub open PR search URL for the preferred remote and branch", () => {
    expect(
      buildFindChangeRequestUrl(
        [remote("origin", "https://github.com/acme/project.git")],
        "feature/api",
        "origin",
      ),
    ).toEqual({
      provider: "github",
      remoteName: "origin",
      url: "https://github.com/acme/project/pulls?q=is%3Apr+is%3Aopen+head%3Afeature%2Fapi",
    });
  });

  it("falls back to a supported GitHub remote", () => {
    expect(
      buildFindChangeRequestUrl(
        [
          remote("mirror", "https://example.com/acme/project.git"),
          remote("origin", "git@github.com:acme/project.git"),
        ],
        "feature/api",
        "mirror",
      )?.remoteName,
    ).toBe("origin");
  });

  it("returns null for non-GitHub providers and missing branches", () => {
    expect(
      buildFindChangeRequestUrl(
        [remote("origin", "https://gitlab.com/acme/project.git")],
        "feature/api",
        "origin",
      ),
    ).toBeNull();
    expect(
      buildFindChangeRequestUrl(
        [remote("origin", "https://github.com/acme/project.git")],
        null,
        "origin",
      ),
    ).toBeNull();
  });
});
