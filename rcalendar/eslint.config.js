import js from "@eslint/js";
import tseslint from "typescript-eslint";
import solid from "eslint-plugin-solid/configs/typescript";

export default tseslint.config(
  // Global ignores — must live in their own object (no other keys) to apply
  // repo-wide, including to generated build output.
  {
    ignores: [
      "**/node_modules/**",
      "**/dist/**",
      "**/target/**",
      "**/src-tauri/**",
      "**/src-tauri/gen/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  // eslint-plugin-solid ships flat configs as a single { plugins, rules }
  // object with no `files` filter — scope them to JSX files.
  {
    ...solid,
    files: ["**/*.{jsx,tsx}"],
  },
  {
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
);
