import js from "@eslint/js";
import tseslint from "typescript-eslint";

const sharedRules = {
  ...js.configs.recommended.rules,
  "no-undef": "off",
  "no-unused-vars": "off",
  "no-eval": "error",
  "no-implied-eval": "error",
  "no-new-func": "error",
  "no-debugger": "error",
  "no-empty": ["error", { allowEmptyCatch: true }],
  eqeqeq: ["error", "always"],
};

export default [
  {
    ignores: ["**/node_modules/**", "**/dist/**", "**/api-snapshots/**"],
  },
  {
    files: ["**/*.{js,mjs,cjs}"],
    languageOptions: { ecmaVersion: "latest", sourceType: "module" },
    rules: sharedRules,
  },
  {
    files: ["**/*.{ts,mts,cts,tsx}"],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: { ecmaVersion: "latest", sourceType: "module" },
    },
    plugins: { "@typescript-eslint": tseslint.plugin },
    rules: {
      ...sharedRules,
      "no-dupe-class-members": "off",
      "no-redeclare": "off",
      "@typescript-eslint/no-duplicate-enum-values": "error",
    },
  },
];
