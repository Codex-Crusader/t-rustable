// Shortcuts (ARCHITECTURE section 12). The app defaults come first, then the user file
// keymap.json, then the suggestions in mod manifests. The user file always wins. A mod
// suggestion that conflicts with a binding is not bound.
//
// keymap.json: { "bindings": { "<action ID>": "Ctrl+Shift+R" | null } }
// An action ID is "app.<name>" or "<mod ID>/<action ID>". null removes the binding.

export type ActionId = string;

export interface Keymap {
  byKey: Map<string, ActionId>;
  byAction: Map<ActionId, string>;
  /** Mod suggestions that are not valid or that conflict with another binding. */
  notBound: ActionId[];
  /** Problems in the user file, for a notice. */
  problems: string[];
}

export const DEFAULT_KEYS: Record<ActionId, string> = {
  "app.open": "Ctrl+O",
  "app.save": "Ctrl+S",
  "app.save-as": "Ctrl+Shift+S",
  "app.close": "Ctrl+W",
  "app.quit": "Ctrl+Q",
  "app.undo": "Ctrl+Z",
  "app.redo": "Ctrl+Y",
  "app.zoom-in": "Ctrl+=",
  "app.zoom-out": "Ctrl+-",
  "app.zoom-reset": "Ctrl+0",
  "app.palette": "Ctrl+K",
  "app.sidebar": "F4",
  "app.panels": "F6",
};

const MODIFIERS = ["Ctrl", "Alt", "Shift", "Meta"] as const;

const MODIFIER_NAMES: Record<string, (typeof MODIFIERS)[number]> = {
  ctrl: "Ctrl",
  control: "Ctrl",
  alt: "Alt",
  option: "Alt",
  shift: "Shift",
  meta: "Meta",
  cmd: "Meta",
  command: "Meta",
  super: "Meta",
  win: "Meta",
};

const KEY_NAMES: Record<string, string> = {
  escape: "Escape",
  esc: "Escape",
  enter: "Enter",
  tab: "Tab",
  space: "Space",
  backspace: "Backspace",
  delete: "Delete",
  del: "Delete",
  insert: "Insert",
  home: "Home",
  end: "End",
  pageup: "PageUp",
  pagedown: "PageDown",
  arrowup: "ArrowUp",
  arrowdown: "ArrowDown",
  arrowleft: "ArrowLeft",
  arrowright: "ArrowRight",
  up: "ArrowUp",
  down: "ArrowDown",
  left: "ArrowLeft",
  right: "ArrowRight",
  plus: "=",
};

function keyName(raw: string): string | null {
  if (raw.length === 1) return raw === "+" ? "=" : raw.toUpperCase();
  const lower = raw.toLowerCase();
  if (/^f([1-9]|1[0-9]|2[0-4])$/.test(lower)) return lower.toUpperCase();
  return KEY_NAMES[lower] ?? null;
}

function join(mods: Set<string>, key: string): string | null {
  // A shortcut needs Ctrl, Alt, or Meta, or an F key, so it never takes typed text.
  const ok = mods.has("Ctrl") || mods.has("Alt") || mods.has("Meta") || /^F\d+$/.test(key);
  return ok ? [...MODIFIERS.filter((m) => mods.has(m)), key].join("+") : null;
}

/** The canonical form of a shortcut such as "ctrl+shift+r", or null if it is not valid. */
export function normalize(text: string): string | null {
  const t = text.trim();
  const parts = t.endsWith("++") ? [...t.slice(0, -2).split("+"), "+"] : t.split("+");
  const raw = parts.pop()?.trim() ?? "";
  const mods = new Set<string>();
  for (const p of parts) {
    const m = MODIFIER_NAMES[p.trim().toLowerCase()];
    if (!m || mods.has(m)) return null;
    mods.add(m);
  }
  const key = keyName(raw);
  return key ? join(mods, key) : null;
}

const CODES: Record<string, string> = {
  Equal: "=",
  Minus: "-",
  NumpadAdd: "=",
  NumpadSubtract: "-",
};

/** The canonical shortcut of a key event. Letters and digits come from the physical
 * key, so Shift and the keyboard layout do not change them. */
export function fromEvent(e: {
  key: string;
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}): string | null {
  if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return null;
  const m = /^(?:Key([A-Z])|Digit(\d))$/.exec(e.code);
  const key = m ? (m[1] ?? m[2]!) : (CODES[e.code] ?? keyName(e.key === " " ? "Space" : e.key));
  if (!key) return null;
  const mods = new Set<string>();
  if (e.ctrlKey) mods.add("Ctrl");
  if (e.altKey) mods.add("Alt");
  if (e.shiftKey) mods.add("Shift");
  if (e.metaKey) mods.add("Meta");
  return join(mods, key);
}

function parseUser(text: string | null, problems: string[]): Map<ActionId, string | null> {
  const out = new Map<ActionId, string | null>();
  if (text === null) return out;
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch (e) {
    problems.push(`keymap.json is not valid JSON: ${(e as Error).message}`);
    return out;
  }
  const bindings = (data as { bindings?: unknown } | null)?.bindings;
  if (typeof bindings !== "object" || bindings === null || Array.isArray(bindings)) {
    problems.push('keymap.json must have a "bindings" object.');
    return out;
  }
  for (const [action, value] of Object.entries(bindings)) {
    if (value === null) {
      out.set(action, null);
      continue;
    }
    const key = typeof value === "string" ? normalize(value) : null;
    if (key) out.set(action, key);
    else problems.push(`keymap.json: the shortcut of "${action}" is not valid. A shortcut needs Ctrl, Alt, or Meta, or an F key.`);
  }
  return out;
}

export function buildKeymap(
  defaults: Record<ActionId, string>,
  userText: string | null,
  suggestions: Record<ActionId, string>,
): Keymap {
  const map: Keymap = { byKey: new Map(), byAction: new Map(), notBound: [], problems: [] };
  const unbind = (action: ActionId) => {
    const key = map.byAction.get(action);
    if (key !== undefined) map.byKey.delete(key);
    map.byAction.delete(action);
  };
  const bind = (action: ActionId, key: string) => {
    const other = map.byKey.get(key);
    if (other !== undefined) unbind(other);
    unbind(action);
    map.byKey.set(key, action);
    map.byAction.set(action, key);
  };

  for (const [action, text] of Object.entries(defaults)) {
    const key = normalize(text);
    if (key) bind(action, key);
  }
  const user = parseUser(userText, map.problems);
  for (const [action, key] of user) {
    if (key === null) unbind(action);
    else bind(action, key);
  }
  for (const [action, text] of Object.entries(suggestions)) {
    if (user.has(action) || map.byAction.has(action)) continue;
    const key = normalize(text);
    if (key && !map.byKey.has(key)) bind(action, key);
    else map.notBound.push(action);
  }
  return map;
}
