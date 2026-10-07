// Checks every locale against English: syntax errors, missing keys and
// keys that no longer exist in English. Run with `bun run i18n:check`.

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { Junk, Message, parse } from "@fluent/syntax";

const root = join(import.meta.dir, "../../locales");
const SOURCE = "en";

function keysOf(code: string): { keys: Set<string>; errors: string[] } {
  const keys = new Set<string>();
  const errors: string[] = [];
  for (const file of readdirSync(join(root, code)).filter((f) => f.endsWith(".ftl"))) {
    const ast = parse(readFileSync(join(root, code, file), "utf8"), {});
    for (const entry of ast.body) {
      if (entry instanceof Message) keys.add(entry.id.name);
      if (entry instanceof Junk)
        errors.push(`${code}/${file}: syntax error near "${entry.content.trim().slice(0, 40)}"`);
    }
  }
  return { keys, errors };
}

const languages: { code: string }[] = JSON.parse(readFileSync(join(root, "languages.json"), "utf8"));
const source = keysOf(SOURCE);
const problems = [...source.errors];

for (const { code } of languages) {
  if (code === SOURCE) continue;
  const { keys, errors } = keysOf(code);
  problems.push(...errors);
  const missing = [...source.keys].filter((k) => !keys.has(k));
  const unused = [...keys].filter((k) => !source.keys.has(k));
  // Missing translations fall back to English, so they are reported but don't fail.
  if (missing.length) console.warn(`${code}: ${missing.length} missing (falls back to English): ${missing.join(", ")}`);
  if (unused.length) problems.push(`${code}: keys not in English: ${unused.join(", ")}`);
}

const dirs = readdirSync(root, { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => d.name);
for (const dir of dirs) {
  if (!languages.some((l) => l.code === dir)) problems.push(`locales/${dir} is not listed in languages.json`);
}

if (problems.length) {
  for (const p of problems) console.error(`✗ ${p}`);
  process.exit(1);
}
console.log(`✓ ${languages.length} languages, ${source.keys.size} messages checked`);
