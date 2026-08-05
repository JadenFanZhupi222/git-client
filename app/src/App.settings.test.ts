import ts from "typescript";
import { describe, expect, it } from "vitest";
import appSource from "./App.tsx?raw";

const EXPECTED_ENTRY_POINTS = {
  "command:github:token": "githubCommand",
  "command:gitlab:token": "gitlabCommand",
  "component:GithubCreatePrDialog": "githubCreatePrDialog",
  "component:GithubPrPanel": "githubPrPanel",
  "component:GitlabCreateMrDialog": "gitlabCreateMrDialog",
  "component:GitlabMrPanel": "gitlabMrPanel",
} as const;

describe("App settings integration", () => {
  it("routes all six legacy credential entry points through the settings lookup", () => {
    const integration = inspectSettingsIntegration(appSource);

    expect(integration.openSettingsUsesLookup).toBe(true);
    expect(integration.entryPoints).toEqual(EXPECTED_ENTRY_POINTS);
  });

  it.each(Object.entries(EXPECTED_ENTRY_POINTS))(
    "detects incorrect wiring for %s",
    (sourceName, entryPoint) => {
      const incorrectlyWired = appSource.replace(
        `openSettingsFor("${entryPoint}")`,
        'openSettingsFor("moreMenu")',
      );

      expect(inspectSettingsIntegration(incorrectlyWired).entryPoints).not.toEqual(
        EXPECTED_ENTRY_POINTS,
      );
      expect(inspectSettingsIntegration(appSource).entryPoints[sourceName]).toBe(entryPoint);
    },
  );

  it("has no legacy token dialog imports, state, or rendering", () => {
    const integration = inspectSettingsIntegration(appSource);
    const { identifiers } = integration;

    expect(integration.imports).not.toContain("./components/GitHubTokenDialog");
    expect(integration.imports).not.toContain("./components/GitLabTokenDialog");
    expect(identifiers).not.toContain("GitHubTokenDialog");
    expect(identifiers).not.toContain("GitLabTokenDialog");
    expect(identifiers).not.toContain("githubTokenOpen");
    expect(identifiers).not.toContain("gitlabTokenOpen");
    expect(identifiers).not.toContain("setGithubTokenOpen");
    expect(identifiers).not.toContain("setGitlabTokenOpen");
  });
});

function inspectSettingsIntegration(source: string) {
  const sourceFile = ts.createSourceFile(
    "App.tsx",
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TSX,
  );
  const entryPoints: Record<string, string> = {};
  const identifiers: string[] = [];
  const imports: string[] = [];
  let openSettingsUsesLookup = false;

  visit(sourceFile);
  return { entryPoints, identifiers, imports, openSettingsUsesLookup };

  function visit(node: ts.Node): void {
    if (ts.isIdentifier(node)) identifiers.push(node.text);
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
      imports.push(node.moduleSpecifier.text);
    }

    if (ts.isFunctionDeclaration(node) && node.name?.text === "openSettingsFor") {
      openSettingsUsesLookup = hasLookupAssignment(node);
    }

    if (ts.isObjectLiteralExpression(node)) {
      const id = stringProperty(node, "id");
      if (id === "github:token" || id === "gitlab:token") {
        const run = propertyInitializer(node, "run");
        const entryPoint = run && directOpenSettingsArgument(run);
        if (entryPoint) recordEntryPoint(`command:${id}`, entryPoint);
      }
    }

    if (ts.isJsxOpeningLikeElement(node)) {
      const component = node.tagName.getText(sourceFile);
      if (
        component === "GithubCreatePrDialog" ||
        component === "GithubPrPanel" ||
        component === "GitlabCreateMrDialog" ||
        component === "GitlabMrPanel"
      ) {
        const callback = jsxExpression(node, "onConfigureToken");
        const entryPoint = callback && directOpenSettingsArgument(callback);
        if (entryPoint) recordEntryPoint(`component:${component}`, entryPoint);
      }
    }

    ts.forEachChild(node, visit);
  }

  function recordEntryPoint(sourceName: string, entryPoint: string): void {
    entryPoints[sourceName] = sourceName in entryPoints ? "<duplicate>" : entryPoint;
  }
}

function hasLookupAssignment(node: ts.FunctionDeclaration): boolean {
  if (node.parameters.length !== 1 || !ts.isIdentifier(node.parameters[0].name)) return false;
  const parameter = node.parameters[0].name.text;
  const statement = node.body?.statements[0];
  if (!statement || !ts.isExpressionStatement(statement)) return false;
  const expression = statement.expression;
  if (!ts.isCallExpression(expression) || expression.expression.getText() !== "setSettingsSection") {
    return false;
  }
  const lookup = expression.arguments[0];
  return Boolean(
    lookup &&
      ts.isCallExpression(lookup) &&
      lookup.expression.getText() === "settingsSectionForEntryPoint" &&
      lookup.arguments.length === 1 &&
      lookup.arguments[0].getText() === parameter,
  );
}

function stringProperty(node: ts.ObjectLiteralExpression, name: string): string | undefined {
  const initializer = propertyInitializer(node, name);
  return initializer && ts.isStringLiteral(initializer) ? initializer.text : undefined;
}

function propertyInitializer(
  node: ts.ObjectLiteralExpression,
  name: string,
): ts.Expression | undefined {
  const property = node.properties.find(
    (candidate): candidate is ts.PropertyAssignment =>
      ts.isPropertyAssignment(candidate) && candidate.name.getText() === name,
  );
  return property?.initializer;
}

function jsxExpression(
  node: ts.JsxOpeningLikeElement,
  name: string,
): ts.Expression | undefined {
  const attribute = node.attributes.properties.find(
    (candidate): candidate is ts.JsxAttribute =>
      ts.isJsxAttribute(candidate) && candidate.name.getText() === name,
  );
  return attribute?.initializer && ts.isJsxExpression(attribute.initializer)
    ? attribute.initializer.expression
    : undefined;
}

function directOpenSettingsArgument(node: ts.Node): string | undefined {
  const body = ts.isArrowFunction(node) || ts.isFunctionExpression(node) ? node.body : node;
  const expressions = ts.isBlock(body)
    ? body.statements
        .filter(ts.isExpressionStatement)
        .map((statement) => statement.expression)
    : [body];
  const calls = expressions.filter(
    (expression): expression is ts.CallExpression =>
      ts.isCallExpression(expression) && expression.expression.getText() === "openSettingsFor",
  );
  if (calls.length !== 1 || calls[0].arguments.length !== 1) return undefined;
  const argument = calls[0].arguments[0];
  return ts.isStringLiteral(argument) ? argument.text : undefined;
}
