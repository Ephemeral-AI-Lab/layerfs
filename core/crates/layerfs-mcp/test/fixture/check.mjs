// The scenario's test: two pages, each linking to the other.
import { readFileSync } from "node:fs";

const read = (name) => {
  try {
    return readFileSync(new URL(`./${name}`, import.meta.url), "utf8");
  } catch {
    return null;
  }
};

const index = read("index.html");
const about = read("about.html");
const problems = [];
if (index === null) problems.push("index.html is missing");
if (about === null) problems.push("about.html is missing");
if (index !== null && !index.includes('href="about.html"')) problems.push("index.html does not link to about.html");
if (about !== null && !about.includes('href="index.html"')) problems.push("about.html does not link to index.html");

if (problems.length > 0) {
  console.error(problems.join("\n"));
  process.exit(1);
}
console.log("check: 2 pages, 2 links");
