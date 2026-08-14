// stylelint plugin — `quill/no-raw-values` (Epic 2.1).
//
// Fails on raw `#hex` color literals and raw `px` length literals in any CSS
// file outside `src/styles/tokens.css`. Components must consume `var(--…)`
// tokens; the only place raw values belong is the token file (shared values
// on `:root`, theme-varying values inside `[data-theme]` blocks).
//
// `var(--…)` references are stripped before matching so token names that
// happen to contain digits or "px" (e.g. `--radius-5px`) don't trip the rule.
import stylelint from "stylelint";

const { createPlugin, utils } = stylelint;

const ruleName = "quill/no-raw-values";

const messages = utils.ruleMessages(ruleName, {
  hex: "Raw hex color in component CSS — define it in src/styles/tokens.css and use var(--…).",
  named:
    "Named color in component CSS — components must not assume a light background (Epic 2.4); use var(--…).",
  px: "Raw px length in component CSS — define it in src/styles/tokens.css and use var(--…).",
});

const HEX = /#[0-9a-f]{3,8}\b/i;
const NAMED = /\b(black|white)\b/i;
const PX = /(^|[^-\w])(\d+(?:\.\d+)?)px\b/;

const rule = () => {
  return (root, result) => {
    const from = result.opts?.from ?? "";
    if (/tokens\.css$/.test(from)) return;

    root.walkDecls((decl) => {
      const value = decl.value.replace(/var\([^)]*\)/g, "");
      if (HEX.test(value)) {
        utils.report({ message: messages.hex, node: decl, result, ruleName });
      }
      if (NAMED.test(value)) {
        utils.report({ message: messages.named, node: decl, result, ruleName });
      }
      if (PX.test(value)) {
        utils.report({ message: messages.px, node: decl, result, ruleName });
      }
    });
  };
};

rule.ruleName = ruleName;
rule.messages = messages;
rule.meta = {
  url: "https://github.com/team/quill/blob/main/CONTRIBUTING.md",
};

export default createPlugin(ruleName, rule);
