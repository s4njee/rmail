import { formatBytes } from "../lib/format";
import { useTheme } from "../lib/theme";
import {
  connectivityText,
  useConnectivity,
  useFootprintBytes,
} from "../lib/store-events";
import "./Titlebar.css";

// In-app titlebar strip (Epic 1.2 / 4.3). Both readouts are live from
// pushed state, never literals: the Banded pill shows connectivity, the
// Hairline readout shows the on-disk footprint only (D2 — no memory figure).
// Treatment differences follow Epic 2.2.
export function Titlebar() {
  const theme = useTheme();
  const connectivity = useConnectivity();
  const footprintBytes = useFootprintBytes();

  return (
    <header class="titlebar">
      <span class="titlebar__app">Quill</span>
      {theme() === "banded" ? (
        <span
          class="titlebar__pill"
          role="status"
          data-connectivity={connectivity().state}
        >
          <span class="titlebar__dot" aria-hidden="true" />
          <span class="titlebar__status">
            {connectivityText(connectivity())}
          </span>
        </span>
      ) : (
        <span class="titlebar__status mono tabular" role="status">
          {formatBytes(footprintBytes())} local
        </span>
      )}
    </header>
  );
}
