import { readFile } from "node:fs/promises";
import path from "node:path";
import MarkdownIt from "markdown-it";

const markdown = new MarkdownIt({ html: false, linkify: false, typographer: false });

function within(filename, root) {
  const relative = path.relative(root, filename);
  return (
    relative === "" ||
    (!relative.startsWith(`..${path.sep}`) &&
      relative !== ".." &&
      !path.isAbsolute(relative))
  );
}

function githubSlug(value) {
  return value
    .trim()
    .toLowerCase()
    .replaceAll(/[^\p{L}\p{N}\p{M} _-]/gu, "")
    .replaceAll(/\s+/gu, "-");
}

function headingAnchors(source) {
  const tokens = markdown.parse(source, {});
  const anchors = new Set();
  const counts = new Map();
  for (let index = 0; index < tokens.length; index += 1) {
    if (tokens[index].type !== "heading_open") continue;
    const content = tokens[index + 1]?.content ?? "";
    const base = githubSlug(content);
    const count = counts.get(base) ?? 0;
    counts.set(base, count + 1);
    anchors.add(count === 0 ? base : `${base}-${count}`);
  }
  return anchors;
}

function tokenTargets(tokens) {
  const targets = [];
  const visit = (items) => {
    for (const token of items) {
      if (token.type === "link_open") targets.push(token.attrGet("href"));
      else if (token.type === "image") targets.push(token.attrGet("src"));
      if (token.children) visit(token.children);
    }
  };
  visit(tokens);
  return targets.filter(Boolean);
}

function unresolvedReferenceLabels(tokens, references) {
  const labels = [];
  const visit = (items) => {
    for (const token of items) {
      if (token.type === "text")
        for (const match of token.content.matchAll(/!?\[([^\]]+)\]\[([^\]]*)\]/gu)) {
          const label = (match[2] || match[1])
            .trim()
            .replaceAll(/\s+/gu, " ")
            .toUpperCase();
          if (!Object.hasOwn(references, label)) labels.push(label);
        }
      if (token.children) visit(token.children);
    }
  };
  visit(tokens);
  return labels;
}

export async function documentationLinkErrors(source, filename, options = {}) {
  const repositoryRoot = path.resolve(options.root ?? path.dirname(filename));
  const read = options.read ?? readFile;
  const errors = [];
  const environment = {};
  const tokens = markdown.parse(source, environment);
  const targets = tokenTargets(tokens);
  for (const reference of Object.values(environment.references ?? {}))
    targets.push(reference.href);

  for (const label of unresolvedReferenceLabels(tokens, environment.references ?? {}))
    errors.push(`undefined reference [${label.toLowerCase()}]`);

  const cache = new Map([[path.resolve(filename), source]]);
  for (const raw of new Set(targets)) {
    if (/^[a-z][a-z0-9+.-]*:/iu.test(raw)) continue;
    const hashIndex = raw.indexOf("#");
    const encodedTarget = hashIndex < 0 ? raw : raw.slice(0, hashIndex);
    const encodedFragment = hashIndex < 0 ? "" : raw.slice(hashIndex + 1);
    const decodedTarget = decodeURIComponent(encodedTarget);
    const target = path.resolve(
      path.dirname(filename),
      decodedTarget || path.basename(filename),
    );
    if (!within(target, repositoryRoot)) {
      errors.push(`target escapes repository: ${raw}`);
      continue;
    }
    let targetSource = cache.get(target);
    if (targetSource === undefined) {
      try {
        targetSource = await read(target, "utf8");
        cache.set(target, targetSource);
      } catch {
        errors.push(`missing target ${decodedTarget || path.basename(filename)}`);
        continue;
      }
    }
    if (encodedFragment && path.extname(target).toLowerCase() === ".md") {
      const fragment = decodeURIComponent(encodedFragment).toLowerCase();
      if (!headingAnchors(targetSource).has(fragment))
        errors.push(
          `missing anchor #${decodeURIComponent(encodedFragment)} in ${decodedTarget || path.basename(filename)}`,
        );
    }
  }
  return errors;
}
