// stylelint flat config (stylelint ≥16).
import noRawValues from "./scripts/stylelint-no-raw-values.mjs";

export default {
  plugins: [noRawValues],
  rules: {
    "quill/no-raw-values": true,
  },
};
