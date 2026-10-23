#!/usr/bin/env node
// Offline stand-in for the Next.js CLI in the LayerFS demo fixture. It follows
// the SDK simulator's rules: build lints JSX text for an unescaped apostrophe
// and writes .next/ with a new build id, start and dev serve on port 3000,
// test fails on an unresolved "@/" import.
"use strict";
const fs = require("fs");
const path = require("path");
const http = require("http");
const crypto = require("crypto");

const root = process.cwd();
const sub = process.argv[2];
const SKIP = /^(node_modules|\.next|packages|\.git)(\/|$)/;

function files(pattern) {
  return fs.readdirSync(root, { recursive: true })
    .map((p) => p.split(path.sep).join("/"))
    .filter((p) => !SKIP.test(p) && pattern.test(p) && fs.statSync(path.join(root, p)).isFile())
    .sort();
}
const pages = () => files(/^app(\/.*)?\/page\.tsx$/);
const h1 = (text) => { const m = text.match(/<h1>(.*?)<\/h1>/); return m ? m[1] : ""; };
const nameOf = (p) => (p === "app/page.tsx" ? "index" : p.slice(4, -9));

function build() {
  for (const p of pages()) {
    const lines = fs.readFileSync(path.join(root, p), "utf8").split("\n");
    for (let i = 0; i < lines.length; i++) {
      const m = lines[i].match(/>([^<>{}]*'[^<>{}]*)</);
      if (m) {
        console.error(`Failed to compile.\n\n./${p}\n${i + 1}:${lines[i].indexOf(m[1]) + 1}  Error: \`'\` can be escaped with \`&apos;\`.  react/no-unescaped-entities`);
        process.exit(1);
      }
    }
  }
  const out = path.join(root, ".next/server/app");
  fs.mkdirSync(out, { recursive: true });
  // Like Next.js, every build gets a new random id.
  fs.writeFileSync(path.join(root, ".next/BUILD_ID"), `b${pages().length}-${crypto.randomBytes(6).toString("base64url")}`);
  for (const p of pages()) {
    const target = path.join(out, `${nameOf(p)}.html`);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, `<h1>${h1(fs.readFileSync(path.join(root, p), "utf8"))}</h1>`);
  }
  console.log(`▲ Next.js 15.0.3\n✓ Compiled successfully\n✓ Generating static pages (${pages().length}/${pages().length})`);
}

function serve(dev) {
  if (!dev && !fs.existsSync(path.join(root, ".next/BUILD_ID"))) {
    console.error("Error: Could not find a production build in the '.next' directory. Try building your app with 'next build'.");
    process.exit(1);
  }
  const port = Number(process.env.PORT || 3000);
  http.createServer((request, response) => {
    const route = request.url.split("?")[0];
    const name = route === "/" ? "index" : route.slice(1);
    const source = dev ? path.join(root, `app${route === "/" ? "" : route}/page.tsx`) : path.join(root, `.next/server/app/${name}.html`);
    if (!fs.existsSync(source)) {
      response.writeHead(404);
      response.end("<h1>404: This page could not be found.</h1>\n");
      return;
    }
    const text = fs.readFileSync(source, "utf8");
    response.end((dev ? `<h1>${h1(text)}</h1>` : text) + "\n");
  }).listen(port, () => console.log(`▲ Next.js 15.0.3${dev ? " (dev)" : ""}\n- Local: http://localhost:${port}\n✓ Ready`));
}

function test() {
  for (const p of files(/\.tsx?$/)) {
    for (const m of fs.readFileSync(path.join(root, p), "utf8").matchAll(/from "@\/([^"]+)"/g)) {
      if (!fs.existsSync(path.join(root, `${m[1]}.ts`)) && !fs.existsSync(path.join(root, `${m[1]}.tsx`))) {
        console.error(`FAIL  ./${p}\n  Cannot find module '@/${m[1]}'\nTests: 1 failed`);
        process.exit(1);
      }
    }
  }
  console.log("PASS  app/page.test.tsx\nTests: 1 passed");
}

if (sub === "build") build();
else if (sub === "start" || sub === "dev") serve(sub === "dev");
else if (sub === "test") test();
else { console.error(`next: unknown command ${sub}`); process.exit(1); }
