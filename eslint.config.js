// ESLint flat config (ESLint 9).
//
// Solid conventions are enforced mechanically here (Epic 1.1): the Solid
// plugin's reactivity rules — no destructured props, no `.map()` where `<For>`
// belongs, no signal read outside a tracking scope — are enabled, and the
// warn-level ones fail the build via `--max-warnings 0` in `pnpm build`.
import tseslint from "typescript-eslint";
import solid from "eslint-plugin-solid";

export default tseslint.config(
  {
    ignores: [
      "dist/**",
      "node_modules/**",
      "public/**",
      "target/**",
      "crates/**",
      "rcalendar/**",
      "src-tauri/**",
      "design_handoff_quill_mail/**",
      "src/lib/ipc/**", // generated from the Rust contract (Epic 3.1)
      "*.config.*",
      "*.cjs",
      "*.mjs",
    ],
  },
  // TypeScript parser + recommended TS rules.
  ...tseslint.configs.recommended,
  // Solid plugin + its full rule set (flat/typescript bundles the
  // recommended rules plus the TS-aware ones).
  solid.configs["flat/typescript"],
  {
    rules: {
      // Explicitness is worth more than brevity in a fresh codebase; the
      // scaffold's tsconfig already forbids unused locals/params, mirror that
      // in lint so editors and CI agree.
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_" },
      ],
    },
  },
);
