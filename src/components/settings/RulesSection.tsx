import { createSignal, For, Show } from "solid-js";
import type { MailRule } from "../../lib/ipc/MailRule";
import type { RuleAction } from "../../lib/ipc/RuleAction";
import type { RuleCondition } from "../../lib/ipc/RuleCondition";
import type { RuleField } from "../../lib/ipc/RuleField";
import type { RuleMatchMode } from "../../lib/ipc/RuleMatchMode";
import type { RuleOperator } from "../../lib/ipc/RuleOperator";
import type { RulePreviewResult } from "../../lib/ipc/RulePreviewResult";
import { useAccounts, useFolders } from "../../lib/mail";
import { updateSettings, useSettings } from "../../lib/settings";
import {
  applyRulesToFolder,
  exportSieveScript,
  parseSieveScript,
  previewRules,
  revertRules,
} from "../../lib/tauri";
import { Modal } from "../Modal";
import "../Settings.css";

const FIELD_OPTIONS: { value: RuleField; label: string }[] = [
  { value: "from", label: "From (Sender)" },
  { value: "to", label: "To (Recipient)" },
  { value: "cc", label: "Cc (Recipient)" },
  { value: "subject", label: "Subject" },
  { value: "listId", label: "List-ID" },
  { value: "hasAttachment", label: "Has Attachment" },
  { value: "body", label: "Body / Snippet" },
];

const OPERATOR_OPTIONS: { value: RuleOperator; label: string }[] = [
  { value: "contains", label: "contains" },
  { value: "notContains", label: "does not contain" },
  { value: "equals", label: "equals" },
  { value: "notEquals", label: "does not equal" },
  { value: "startsWith", label: "starts with" },
  { value: "endsWith", label: "ends with" },
  { value: "matches", label: "matches wildcard (*)" },
];

type ActionTypeKey =
  | "moveToFolder"
  | "markRead"
  | "markUnread"
  | "markFlagged"
  | "markUnflagged"
  | "delete"
  | "archive";

const ACTION_OPTIONS: { value: ActionTypeKey; label: string }[] = [
  { value: "moveToFolder", label: "Move to folder" },
  { value: "markRead", label: "Mark as read" },
  { value: "markUnread", label: "Mark as unread" },
  { value: "markFlagged", label: "Star / Flag" },
  { value: "markUnflagged", label: "Unstar / Unflag" },
  { value: "archive", label: "Archive" },
  { value: "delete", label: "Delete (Trash)" },
];

interface EditableAction {
  type: ActionTypeKey;
  folderName: string;
}

export function RulesSection() {
  const settings = useSettings();
  const accounts = useAccounts();
  const folders = useFolders();

  const [editorOpen, setEditorOpen] = createSignal(false);
  const [editingRuleId, setEditingRuleId] = createSignal<string | null>(null);

  // Editor form state
  const [ruleName, setRuleName] = createSignal("");
  const [matchMode, setMatchMode] = createSignal<RuleMatchMode>("all");
  const [conditions, setConditions] = createSignal<RuleCondition[]>([]);
  const [actions, setActions] = createSignal<EditableAction[]>([]);
  const [stopProcessing, setStopProcessing] = createSignal(true);
  const [ruleEnabled, setRuleEnabled] = createSignal(true);

  // Sieve Modals
  const [importOpen, setImportOpen] = createSignal(false);
  const [importScript, setImportScript] = createSignal("");
  const [importError, setImportError] = createSignal<string | null>(null);

  const [exportOpen, setExportOpen] = createSignal(false);
  const [exportScriptText, setExportScriptText] = createSignal("");
  const [copied, setCopied] = createSignal(false);

  // Status feedback
  const [runStatus, setRunStatus] = createSignal<string | null>(null);
  // P1.3 rule dry-run/preview + undo.
  const [previewResult, setPreviewResult] =
    createSignal<RulePreviewResult | null>(null);
  const [previewing, setPreviewing] = createSignal(false);
  const [previewError, setPreviewError] = createSignal("");
  const [selectedFolderToRun, setSelectedFolderToRun] =
    createSignal<string>("Inbox");

  const rulesList = (): MailRule[] => {
    return settings()?.rules || [];
  };

  const openNewRuleModal = () => {
    setEditingRuleId(null);
    setRuleName("New Rule");
    setMatchMode("all");
    setConditions([{ field: "from", operator: "contains", value: "" }]);
    setActions([{ type: "moveToFolder", folderName: "Archive" }]);
    setStopProcessing(true);
    setRuleEnabled(true);
    setEditorOpen(true);
  };

  const openEditRuleModal = (rule: MailRule) => {
    setEditingRuleId(rule.id);
    setRuleName(rule.name);
    setMatchMode(rule.matchMode);
    setConditions(
      rule.conditions.length > 0
        ? rule.conditions.map((c) => ({ ...c }))
        : [{ field: "from", operator: "contains", value: "" }],
    );

    const editableActions: EditableAction[] = rule.actions.map((act) => {
      if (typeof act === "object" && "moveToFolder" in act) {
        return {
          type: "moveToFolder",
          folderName: act.moveToFolder.folderName,
        };
      }
      return { type: act as ActionTypeKey, folderName: "" };
    });

    setActions(
      editableActions.length > 0
        ? editableActions
        : [{ type: "markRead", folderName: "" }],
    );
    setStopProcessing(rule.stopProcessing);
    setRuleEnabled(rule.enabled);
    setEditorOpen(true);
  };

  const saveRule = async () => {
    if (!ruleName().trim()) return;

    const finalActions: RuleAction[] = actions().map((a) => {
      if (a.type === "moveToFolder") {
        return {
          moveToFolder: { folderName: a.folderName.trim() || "Archive" },
        };
      }
      return a.type as RuleAction;
    });

    const newRule: MailRule = {
      id: editingRuleId() || `rule_${Date.now()}`,
      name: ruleName().trim(),
      enabled: ruleEnabled(),
      matchMode: matchMode(),
      conditions: conditions(),
      actions: finalActions,
      stopProcessing: stopProcessing(),
    };

    const current = [...rulesList()];
    if (editingRuleId()) {
      const idx = current.findIndex((r) => r.id === editingRuleId());
      if (idx !== -1) {
        current[idx] = newRule;
      } else {
        current.push(newRule);
      }
    } else {
      current.push(newRule);
    }

    await updateSettings({ rules: current });
    setEditorOpen(false);
  };

  const deleteRule = async (id: string) => {
    const next = rulesList().filter((r) => r.id !== id);
    await updateSettings({ rules: next });
  };

  const toggleRuleEnabled = async (id: string) => {
    const next = rulesList().map((r) => {
      if (r.id === id) {
        return { ...r, enabled: !r.enabled };
      }
      return r;
    });
    await updateSettings({ rules: next });
  };

  const moveRule = async (index: number, direction: "up" | "down") => {
    const list = [...rulesList()];
    const targetIdx = direction === "up" ? index - 1 : index + 1;
    if (targetIdx < 0 || targetIdx >= list.length) return;

    const temp = list[index];
    list[index] = list[targetIdx];
    list[targetIdx] = temp;

    await updateSettings({ rules: list });
  };

  // Condition manipulations
  const addCondition = () => {
    setConditions([
      ...conditions(),
      { field: "subject", operator: "contains", value: "" },
    ]);
  };

  const updateCondition = (index: number, patch: Partial<RuleCondition>) => {
    const list = [...conditions()];
    list[index] = { ...list[index], ...patch };
    setConditions(list);
  };

  const removeCondition = (index: number) => {
    if (conditions().length <= 1) return;
    const list = conditions().filter((_, i) => i !== index);
    setConditions(list);
  };

  // Action manipulations
  const addAction = () => {
    setActions([...actions(), { type: "markRead", folderName: "" }]);
  };

  const updateAction = (index: number, patch: Partial<EditableAction>) => {
    const list = [...actions()];
    list[index] = { ...list[index], ...patch };
    setActions(list);
  };

  const removeAction = (index: number) => {
    if (actions().length <= 1) return;
    const list = actions().filter((_, i) => i !== index);
    setActions(list);
  };

  // Sieve operations
  const handleOpenExport = async () => {
    try {
      const script = await exportSieveScript(rulesList());
      setExportScriptText(script);
      setCopied(false);
      setExportOpen(true);
    } catch (e) {
      console.error("Export Sieve failed:", e);
    }
  };

  const handleCopyExport = async () => {
    try {
      await navigator.clipboard.writeText(exportScriptText());
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      console.error("Copy failed:", e);
    }
  };

  const handleImportSieve = async () => {
    setImportError(null);
    try {
      const parsed = await parseSieveScript(importScript());
      if (!parsed || parsed.length === 0) {
        setImportError(
          'No valid Sieve rules found. Please check syntax (e.g. `if header :contains "from" "user@example.com" { ... }`).',
        );
        return;
      }
      const combined = [...rulesList(), ...parsed];
      await updateSettings({ rules: combined });
      setImportOpen(false);
      setImportScript("");
    } catch (e) {
      setImportError(String(e));
    }
  };

  const targetAccountId = () => accounts()[0]?.id ?? 0;

  // P1.3 dry-run: show what the rules WOULD change, with the rule order, before
  // anything is applied.
  const handlePreviewRules = async () => {
    const folder = selectedFolderToRun();
    setPreviewing(true);
    setPreviewError("");
    setPreviewResult(null);
    try {
      const preview = await previewRules(targetAccountId(), folder);
      setPreviewResult(preview);
    } catch (e) {
      setPreviewError(String(e));
    } finally {
      setPreviewing(false);
    }
  };

  const handleRunRulesNow = async () => {
    const folder = selectedFolderToRun();
    setRunStatus(`Running rules on ${folder}...`);

    try {
      // Capture the before-state for undo (the preview is the undo base).
      const preview = await previewRules(targetAccountId(), folder);
      const count = await applyRulesToFolder(targetAccountId(), folder);
      setPreviewResult(preview);
      setRunStatus(
        `Processed rules on ${folder}: ${count} message(s) modified.`,
      );
      setTimeout(() => setRunStatus(null), 4000);
    } catch (e) {
      setRunStatus(`Error applying rules: ${e}`);
      setTimeout(() => setRunStatus(null), 4000);
    }
  };

  // P1.3 undo: restore each affected message to its before-state.
  const handleUndoRules = async () => {
    const p = previewResult();
    if (!p) return;
    try {
      const reverted = await revertRules(targetAccountId(), p.previews);
      setPreviewResult(null);
      setRunStatus(`Reverted ${reverted} message(s).`);
      setTimeout(() => setRunStatus(null), 4000);
    } catch (e) {
      setPreviewError(String(e));
    }
  };

  const formatConditionSummary = (c: RuleCondition): string => {
    const fLabel =
      FIELD_OPTIONS.find((f) => f.value === c.field)?.label || c.field;
    return `${fLabel} ${c.operator} "${c.value}"`;
  };

  const formatActionSummary = (a: RuleAction): string => {
    if (typeof a === "object" && "moveToFolder" in a) {
      return `Move to "${a.moveToFolder.folderName}"`;
    }
    switch (a) {
      case "markRead":
        return "Mark read";
      case "markUnread":
        return "Mark unread";
      case "markFlagged":
        return "Star";
      case "markUnflagged":
        return "Unstar";
      case "archive":
        return "Archive";
      case "delete":
        return "Delete";
      default:
        return String(a);
    }
  };

  return (
    <div class="rules-section">
      <div class="rules-header-row">
        <div>
          <h3 class="rules-title">Rules & Filters</h3>
          <p class="rules-subtitle">
            Automatically filter, route, and flag incoming mail across your
            accounts.
          </p>
        </div>
        <div class="rules-header-actions">
          <button
            type="button"
            class="rules-btn rules-btn--secondary"
            onClick={() => setImportOpen(true)}
          >
            Import Sieve
          </button>
          <button
            type="button"
            class="rules-btn rules-btn--secondary"
            onClick={handleOpenExport}
          >
            Export Sieve
          </button>
          <button
            type="button"
            class="rules-btn rules-btn--primary"
            onClick={openNewRuleModal}
          >
            + Add Rule
          </button>
        </div>
      </div>

      <div class="rules-run-banner">
        <span class="rules-run-label">Manual Execution:</span>
        <select
          class="rules-select rules-run-select"
          value={selectedFolderToRun()}
          onChange={(e) => setSelectedFolderToRun(e.currentTarget.value)}
        >
          <For each={folders()}>
            {(f) => <option value={f.name}>{f.name}</option>}
          </For>
        </select>
        <button
          type="button"
          class="rules-btn rules-btn--secondary rules-run-btn"
          onClick={() => void handlePreviewRules()}
          disabled={previewing()}
        >
          {previewing() ? "Previewing…" : "Preview"}
        </button>
        <button
          type="button"
          class="rules-btn rules-btn--secondary rules-run-btn"
          onClick={() => void handleRunRulesNow()}
        >
          ▶ Run Rules Now
        </button>
        <Show when={previewResult()}>
          <button
            type="button"
            class="rules-btn rules-btn--secondary rules-run-btn"
            onClick={() => void handleUndoRules()}
          >
            Undo
          </button>
        </Show>
        <Show when={runStatus()}>
          <span class="rules-run-status">{runStatus()}</span>
        </Show>
      </div>

      {/* P1.3 dry-run result — affected count + the matching-rule order. */}
      <Show when={previewResult()}>
        {(p) => (
          <div class="rules-preview">
            <p class="rules-preview__summary">
              Rules would change <strong>{p().affected}</strong> message(s) in{" "}
              {selectedFolderToRun()}.
              {p().affected === 0
                ? " Nothing matches — safe to apply or adjust."
                : " Run Rules Now applies them; Undo reverts."}
            </p>
            <Show when={p().previews.length > 0}>
              <ul class="rules-preview__list">
                <For each={p().previews}>
                  {(pv) => (
                    <li class="rules-preview__item">
                      <div class="rules-preview__message">
                        <span class="rules-preview__subject">
                          {pv.subject || "(no subject)"}
                        </span>
                        <span class="rules-preview__sender">{pv.sender}</span>
                      </div>
                      <ol class="rules-preview__matched">
                        <For each={pv.matched}>
                          {(m) => (
                            <li>
                              <span class="rules-preview__rule">
                                {m.ruleIndex + 1}. {m.ruleName}
                              </span>
                              <span class="rules-preview__actions">
                                {m.actions.join(" · ")}
                              </span>
                            </li>
                          )}
                        </For>
                      </ol>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </div>
        )}
      </Show>
      <Show when={previewError()}>
        <p class="rules-run-status" role="alert">
          {previewError()}
        </p>
      </Show>

      <Show
        when={rulesList().length > 0}
        fallback={
          <div class="rules-empty">
            <p class="rules-empty__text">No filtering rules configured.</p>
            <p class="rules-empty__sub">
              Create rules to automatically organize incoming mail into folders,
              star urgent messages, or mark newsletters as read.
            </p>
            <button
              type="button"
              class="rules-btn rules-btn--primary"
              onClick={openNewRuleModal}
            >
              + Create your first rule
            </button>
          </div>
        }
      >
        <div class="rules-list">
          <For each={rulesList()}>
            {(rule, idx) => (
              <div
                class="rules-item"
                classList={{ "rules-item--disabled": !rule.enabled }}
              >
                <div class="rules-item__reorder">
                  <button
                    type="button"
                    class="rules-icon-btn"
                    disabled={idx() === 0}
                    onClick={() => moveRule(idx(), "up")}
                    aria-label="Move rule up"
                  >
                    ▲
                  </button>
                  <button
                    type="button"
                    class="rules-icon-btn"
                    disabled={idx() === rulesList().length - 1}
                    onClick={() => moveRule(idx(), "down")}
                    aria-label="Move rule down"
                  >
                    ▼
                  </button>
                </div>

                <label class="rules-item__toggle">
                  <input
                    type="checkbox"
                    checked={rule.enabled}
                    onChange={() => toggleRuleEnabled(rule.id)}
                  />
                  <span class="rules-toggle-track" />
                </label>

                <div class="rules-item__content">
                  <div class="rules-item__top">
                    <span class="rules-item__name">{rule.name}</span>
                    <Show when={rule.stopProcessing}>
                      <span class="rules-badge rules-badge--stop">
                        Stop evaluating
                      </span>
                    </Show>
                    <span class="rules-badge rules-badge--mode">
                      {rule.matchMode === "all" ? "Match ALL" : "Match ANY"}
                    </span>
                  </div>

                  <div class="rules-item__summary">
                    <span class="rules-summary-label">If:</span>
                    <span class="rules-summary-value">
                      {rule.conditions.map(formatConditionSummary).join(", ")}
                    </span>
                  </div>

                  <div class="rules-item__summary">
                    <span class="rules-summary-label">Then:</span>
                    <span class="rules-summary-value">
                      {rule.actions.map(formatActionSummary).join(", ")}
                    </span>
                  </div>
                </div>

                <div class="rules-item__actions">
                  <button
                    type="button"
                    class="rules-btn rules-btn--small"
                    onClick={() => openEditRuleModal(rule)}
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    class="rules-btn rules-btn--small rules-btn--danger"
                    onClick={() => deleteRule(rule.id)}
                  >
                    Delete
                  </button>
                </div>
              </div>
            )}
          </For>
        </div>
      </Show>

      {/* Rule Editor Modal */}
      <Show when={editorOpen()}>
        <Modal
          title={editingRuleId() ? "Edit Mail Rule" : "Create Mail Rule"}
          onClose={() => setEditorOpen(false)}
        >
          <div class="rule-editor-modal">
            <div class="rule-field-group">
              <label class="rule-label">Rule Name</label>
              <input
                type="text"
                class="rule-input"
                placeholder="e.g., File newsletters into Archive"
                value={ruleName()}
                onInput={(e) => setRuleName(e.currentTarget.value)}
              />
            </div>

            <div class="rule-field-group">
              <div class="rule-match-header">
                <span class="rule-label">Match Conditions</span>
                <div class="rule-match-mode-selector">
                  <label class="rule-radio-label">
                    <input
                      type="radio"
                      name="matchMode"
                      value="all"
                      checked={matchMode() === "all"}
                      onChange={() => setMatchMode("all")}
                    />
                    All of the following (AND)
                  </label>
                  <label class="rule-radio-label">
                    <input
                      type="radio"
                      name="matchMode"
                      value="any"
                      checked={matchMode() === "any"}
                      onChange={() => setMatchMode("any")}
                    />
                    Any of the following (OR)
                  </label>
                </div>
              </div>

              <div class="rule-builder-list">
                <For each={conditions()}>
                  {(cond, idx) => (
                    <div class="rule-builder-row">
                      <select
                        class="rule-select"
                        value={cond.field}
                        onChange={(e) =>
                          updateCondition(idx(), {
                            field: e.currentTarget.value as RuleField,
                          })
                        }
                      >
                        <For each={FIELD_OPTIONS}>
                          {(f) => <option value={f.value}>{f.label}</option>}
                        </For>
                      </select>

                      <select
                        class="rule-select"
                        value={cond.operator}
                        onChange={(e) =>
                          updateCondition(idx(), {
                            operator: e.currentTarget.value as RuleOperator,
                          })
                        }
                      >
                        <For each={OPERATOR_OPTIONS}>
                          {(op) => <option value={op.value}>{op.label}</option>}
                        </For>
                      </select>

                      <input
                        type="text"
                        class="rule-input rule-input--val"
                        placeholder="Value to match"
                        value={cond.value}
                        onInput={(e) =>
                          updateCondition(idx(), {
                            value: e.currentTarget.value,
                          })
                        }
                      />

                      <button
                        type="button"
                        class="rule-row-remove"
                        disabled={conditions().length <= 1}
                        onClick={() => removeCondition(idx())}
                        title="Remove condition"
                      >
                        ✕
                      </button>
                    </div>
                  )}
                </For>
              </div>

              <button
                type="button"
                class="rule-add-row-btn"
                onClick={addCondition}
              >
                + Add another condition
              </button>
            </div>

            <div class="rule-field-group">
              <label class="rule-label">Perform Actions</label>
              <div class="rule-builder-list">
                <For each={actions()}>
                  {(act, idx) => (
                    <div class="rule-builder-row">
                      <select
                        class="rule-select"
                        value={act.type}
                        onChange={(e) =>
                          updateAction(idx(), {
                            type: e.currentTarget.value as ActionTypeKey,
                          })
                        }
                      >
                        <For each={ACTION_OPTIONS}>
                          {(opt) => (
                            <option value={opt.value}>{opt.label}</option>
                          )}
                        </For>
                      </select>

                      <Show when={act.type === "moveToFolder"}>
                        <select
                          class="rule-select rule-input--val"
                          value={act.folderName}
                          onChange={(e) =>
                            updateAction(idx(), {
                              folderName: e.currentTarget.value,
                            })
                          }
                        >
                          <For each={folders()}>
                            {(f) => <option value={f.name}>{f.name}</option>}
                          </For>
                        </select>
                      </Show>

                      <button
                        type="button"
                        class="rule-row-remove"
                        disabled={actions().length <= 1}
                        onClick={() => removeAction(idx())}
                        title="Remove action"
                      >
                        ✕
                      </button>
                    </div>
                  )}
                </For>
              </div>

              <button
                type="button"
                class="rule-add-row-btn"
                onClick={addAction}
              >
                + Add another action
              </button>
            </div>

            <div class="rule-options-footer">
              <label class="rule-checkbox-label">
                <input
                  type="checkbox"
                  checked={stopProcessing()}
                  onChange={(e) => setStopProcessing(e.currentTarget.checked)}
                />
                Stop evaluating further rules if this rule matches
              </label>

              <label class="rule-checkbox-label">
                <input
                  type="checkbox"
                  checked={ruleEnabled()}
                  onChange={(e) => setRuleEnabled(e.currentTarget.checked)}
                />
                Enable this rule
              </label>
            </div>

            <div class="rule-editor-footer">
              <button
                type="button"
                class="rules-btn rules-btn--secondary"
                onClick={() => setEditorOpen(false)}
              >
                Cancel
              </button>
              <button
                type="button"
                class="rules-btn rules-btn--primary"
                onClick={saveRule}
              >
                Save Rule
              </button>
            </div>
          </div>
        </Modal>
      </Show>

      {/* Sieve Import Modal */}
      <Show when={importOpen()}>
        <Modal
          title="Import Sieve Script (RFC 5228)"
          onClose={() => setImportOpen(false)}
        >
          <div class="rule-sieve-modal">
            <p class="rule-sieve-help">
              Paste standard Sieve code below. Quill will parse header
              conditions, fileinto actions, flags, and stop instructions.
            </p>
            <textarea
              class="rule-sieve-textarea"
              rows={10}
              placeholder={`require ["fileinto", "reject"];\n\nif header :contains "from" "news@example.com" {\n    fileinto "Newsletters";\n    stop;\n}`}
              value={importScript()}
              onInput={(e) => setImportScript(e.currentTarget.value)}
            />
            <Show when={importError()}>
              <p class="rule-sieve-error">{importError()}</p>
            </Show>
            <div class="rule-editor-footer">
              <button
                type="button"
                class="rules-btn rules-btn--secondary"
                onClick={() => setImportOpen(false)}
              >
                Cancel
              </button>
              <button
                type="button"
                class="rules-btn rules-btn--primary"
                onClick={handleImportSieve}
              >
                Import Rules
              </button>
            </div>
          </div>
        </Modal>
      </Show>

      {/* Sieve Export Modal */}
      <Show when={exportOpen()}>
        <Modal title="Export Sieve Script" onClose={() => setExportOpen(false)}>
          <div class="rule-sieve-modal">
            <p class="rule-sieve-help">
              Below is the generated RFC 5228 Sieve script representation of
              your active Quill mail rules.
            </p>
            <pre class="rule-sieve-code">
              <code>{exportScriptText()}</code>
            </pre>
            <div class="rule-editor-footer">
              <button
                type="button"
                class="rules-btn rules-btn--secondary"
                onClick={() => setExportOpen(false)}
              >
                Close
              </button>
              <button
                type="button"
                class="rules-btn rules-btn--primary"
                onClick={handleCopyExport}
              >
                {copied() ? "Copied!" : "Copy to Clipboard"}
              </button>
            </div>
          </div>
        </Modal>
      </Show>
    </div>
  );
}
