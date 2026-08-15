import { createSignal, For, onMount, Show } from "solid-js";
import type { ContactGroup } from "../../lib/ipc/ContactGroup";
import type { ContactSuggestion } from "../../lib/ipc/ContactSuggestion";
import {
  addContactToGroup,
  contactGroupMembers,
  createContactGroup,
  deleteContactGroup,
  listContactGroups,
  removeContactFromGroup,
} from "../../lib/tauri";
import "../Settings.css";

// Settings → Contacts (P1.2): named contact groups. Recipient suggestions
// come from mail history automatically; groups let the user curate a list of
// addresses (a CardDAV import will feed the same local model once that's gated
// in — see backlog P1.2).
export function ContactGroupsSection() {
  const [groups, setGroups] = createSignal<ContactGroup[]>([]);
  const [members, setMembers] = createSignal<
    Record<number, ContactSuggestion[]>
  >({});
  const [newGroup, setNewGroup] = createSignal("");
  const [newMember, setNewMember] = createSignal("");
  const [expanded, setExpanded] = createSignal<number | null>(null);
  const [error, setError] = createSignal("");

  const reload = async () => {
    try {
      setGroups(await listContactGroups());
    } catch (e) {
      setError(String(e));
    }
  };

  onMount(() => void reload());

  const toggleExpanded = async (id: number) => {
    const next = expanded() === id ? null : id;
    setExpanded(next);
    if (next != null) {
      try {
        setMembers({ ...members(), [id]: await contactGroupMembers(id) });
      } catch (e) {
        setError(String(e));
      }
    }
  };

  const create = async () => {
    const name = newGroup().trim();
    if (!name) return;
    try {
      await createContactGroup(name);
      setNewGroup("");
      setError("");
      await reload();
    } catch (e) {
      setError(String(e));
    }
  };

  const remove = async (id: number) => {
    try {
      await deleteContactGroup(id);
      const m = { ...members() };
      delete m[id];
      setMembers(m);
      if (expanded() === id) setExpanded(null);
      await reload();
    } catch (e) {
      setError(String(e));
    }
  };

  const addMember = async (groupId: number) => {
    const addr = newMember().trim();
    if (!addr) return;
    try {
      await addContactToGroup(groupId, addr);
      setNewMember("");
      setMembers({
        ...members(),
        [groupId]: await contactGroupMembers(groupId),
      });
    } catch (e) {
      setError(String(e));
    }
  };

  const removeMember = async (groupId: number, address: string) => {
    try {
      await removeContactFromGroup(groupId, address);
      setMembers({
        ...members(),
        [groupId]: await contactGroupMembers(groupId),
      });
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div class="settings-accounts">
      <p class="settings-note">
        Contact groups organize addresses you've emailed. Recipient suggestions
        still come from your mail history automatically.
      </p>

      <div class="settings-footer">
        <input
          class="contact-group__name-input"
          type="text"
          value={newGroup()}
          onInput={(e) => setNewGroup(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void create();
          }}
          placeholder="New group name"
          aria-label="New group name"
        />
        <button
          type="button"
          class="add-account-btn"
          disabled={!newGroup().trim()}
          onClick={() => void create()}
        >
          Add group
        </button>
      </div>

      <For each={groups()}>
        {(g) => (
          <div class="contact-group">
            <div class="settings-row">
              <button
                type="button"
                class="contact-group__name"
                onClick={() => void toggleExpanded(g.id)}
                aria-expanded={expanded() === g.id}
              >
                {g.name}
                <span class="contact-group__count">
                  {members()[g.id]?.length ?? 0}
                </span>
              </button>
              <button
                type="button"
                class="settings-row__remove"
                onClick={() => void remove(g.id)}
              >
                Delete
              </button>
            </div>
            <Show when={expanded() === g.id}>
              <div class="contact-group__members">
                <For each={members()[g.id] ?? []}>
                  {(m) => (
                    <div class="contact-group__member">
                      <span class="contact-group__member-text">
                        {m.name ? `${m.name} ` : ""}
                        <span class="mono">{m.address}</span>
                      </span>
                      <button
                        type="button"
                        class="settings-row__remove"
                        onClick={() => void removeMember(g.id, m.address)}
                      >
                        Remove
                      </button>
                    </div>
                  )}
                </For>
                <Show when={(members()[g.id] ?? []).length === 0}>
                  <p class="contact-group__empty">No members yet.</p>
                </Show>
                <div class="contact-group__add">
                  <input
                    class="contact-group__name-input"
                    type="text"
                    value={newMember()}
                    onInput={(e) => setNewMember(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void addMember(g.id);
                    }}
                    placeholder="address@example.com"
                    aria-label="Add member address"
                  />
                  <button
                    type="button"
                    class="btn btn--secondary btn--sm"
                    disabled={!newMember().trim()}
                    onClick={() => void addMember(g.id)}
                  >
                    Add
                  </button>
                </div>
              </div>
            </Show>
          </div>
        )}
      </For>

      <Show when={groups().length === 0}>
        <p class="contact-group__empty">
          No contact groups yet — create one above to keep a curated list.
        </p>
      </Show>
      <Show when={error()}>
        <p class="settings-sync-msg" role="alert">
          {error()}
        </p>
      </Show>
    </div>
  );
}
