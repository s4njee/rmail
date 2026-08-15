import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import type { MessageDetail } from "../lib/ipc/MessageDetail";
import { useDark } from "../lib/theme";
import "./MailBody.css";

type MailBodyProps = {
  detail: MessageDetail;
  /** Load this message's remote images (per-sender trust, Epic 7.3). */
  allowImages: boolean;
  /** Add the sender to the trusted-image set and re-render with images on. */
  onLoadImages: () => void;
  /** Always trust images from this sender (Roadmap 3.7). */
  onAlwaysTrustSender?: () => void;
  /** The iframe surfaced a link; the parent shows the destination and opens it. */
  onOpenLink: (url: string) => void;
};

// The injected script is the ONLY script the iframe's CSP will run (it carries
// this nonce; mail HTML never does).
const SCRIPT_NONCE = "quill-mail-handler";

/** The theme's reading-pane background, so the iframe doesn't flash white in
 * either treatment (and follows the dark palette — Epic 2.4 / P1.5). */
function readingBackground(): string {
  return (
    getComputedStyle(document.documentElement)
      .getPropertyValue("--color-reading")
      .trim() || "#fff"
  );
}

/** P1.5: force a light-on-dark default for HTML mail in dark mode, so a
 * message without its own background stays readable. Messages that set their
 * own background/colors keep them (images are never rewritten). */
function darkMailStyles(): string {
  return (
    "color:#dce2ea;" +
    "a{color:#8aa4ff}" +
    "table,tr,td,div,span{color:inherit}" +
    "body{color-scheme:dark}"
  );
}

/**
 * The document rendered inside the sandboxed iframe.
 *
 * Security model (Epic 7.3): the HTML body was sanitized in Rust; the iframe
 * is a second layer. `sandbox="allow-scripts"` is a deliberate deviation from
 * the plan's literal "without allow-scripts" — auto-sizing, link clicks, and
 * image loading all need a handler — but mail scripts remain inert because:
 *   • the sanitizer removed every script/event handler from the body;
 *   • the iframe is opaque-origin (no allow-same-origin), so even a
 *     hypothetical script could not reach the app or Tauri;
 *   • the iframe CSP is `script-src 'nonce-…'` — only the app's handler runs.
 * Remote images are gated by `img-src`: data:/blob: by default, https/http
 * only when the user (per sender) opts in.
 */
function buildSrcdoc(
  sanitizedHtml: string,
  allowImages: boolean,
  dark: boolean,
): string {
  const csp = [
    "default-src 'none'",
    "style-src 'unsafe-inline'",
    `script-src 'nonce-${SCRIPT_NONCE}'`,
    allowImages ? "img-src data: blob: https: http:" : "img-src data: blob:",
    "font-src data:",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
  ].join("; ");

  const script = `
    const ALLOW_IMAGES = ${allowImages ? "true" : "false"};
    function post(msg) { msg.__quill = true; parent.postMessage(msg, "*"); }
    function reportHeight() {
      post({ type: "height", height: document.documentElement.scrollHeight });
    }
    if (ALLOW_IMAGES) {
      document.querySelectorAll("img[data-src]").forEach((img) => {
        img.src = img.getAttribute("data-src");
        img.removeAttribute("data-src");
      });
    }
    document.addEventListener("click", (e) => {
      const a = e.target && e.target.closest ? e.target.closest("a") : null;
      if (!a) return;
      e.preventDefault();
      const href = a.getAttribute("href") || "";
      post({
        type: href.indexOf("mailto:") === 0 ? "mailto" : "open",
        url: href,
        text: a.textContent || "",
      });
    }, true);
    reportHeight();
    if (typeof ResizeObserver !== "undefined") {
      new ResizeObserver(reportHeight).observe(document.documentElement);
    }
    window.addEventListener("load", reportHeight);
  `;

  // Defensive: a stray `</body>`/`</html>` in mail text must not close our
  // structure early.
  const body = sanitizedHtml
    .replace(/<\/body>/gi, "&lt;/body&gt;")
    .replace(/<\/html>/gi, "&lt;/html&gt;");

  return (
    `<!doctype html><html><head><meta charset="utf-8">` +
    `<meta http-equiv="Content-Security-Policy" content="${csp}">` +
    `<style>html,body{margin:0}body{background:${readingBackground()};word-wrap:break-word;${dark ? darkMailStyles() : ""}}</style>` +
    `</head><body>${body}` +
    `<script nonce="${SCRIPT_NONCE}">${script}</script></body></html>`
  );
}

/** The sandboxed iframe that renders a sanitized HTML mail body (Epic 7.3). */
export function MailBody(props: MailBodyProps) {
  const [frameEl, setFrameEl] = createSignal<HTMLIFrameElement | null>(null);
  const [frameHeight, setFrameHeight] = createSignal(120);

  createEffect(() => {
    const onMessage = (event: MessageEvent) => {
      const msg = event.data;
      if (!msg || msg.__quill !== true) return;
      if (event.source !== frameEl()?.contentWindow) return; // only our iframe
      if (msg.type === "height") setFrameHeight(Number(msg.height) || 120);
      else if (msg.type === "open") props.onOpenLink(String(msg.url));
      // "mailto" opens the composer in Epic 13.
    };
    window.addEventListener("message", onMessage);
    onCleanup(() => window.removeEventListener("message", onMessage));
  });

  return (
    <div class="mail-body">
      <Show when={props.detail.remote_image_count > 0 && !props.allowImages}>
        <div class="mail-body__privacy-banner">
          <span class="mail-body__privacy-text">
            Remote images are blocked to protect your privacy.
          </span>
          <div class="mail-body__privacy-actions">
            <button
              type="button"
              class="mail-body__load-images"
              onClick={() => props.onLoadImages()}
            >
              Load images once
            </button>
            <Show when={props.onAlwaysTrustSender}>
              <span class="mail-body__privacy-sep">·</span>
              <button
                type="button"
                class="mail-body__load-images mail-body__trust-sender"
                onClick={() => props.onAlwaysTrustSender?.()}
              >
                Always trust {props.detail.row.sender_address}
              </button>
            </Show>
          </div>
        </div>
      </Show>
      <iframe
        ref={setFrameEl}
        class="mail-body__frame"
        title="Message body"
        sandbox="allow-scripts"
        srcdoc={buildSrcdoc(
          props.detail.body_html ?? "",
          props.allowImages,
          useDark()(),
        )}
        style={{ height: `${frameHeight()}px` }}
      />
    </div>
  );
}
