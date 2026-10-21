// Data from the Rust side. Positions are display points at zoom 1 (see wire.rs).

export type DocId = number;
export type PageId = string;

export interface Box2 {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface PageInfo {
  id: PageId;
  label: string;
  width: number;
  height: number;
  revision: number;
}

export interface DocInfo {
  id: DocId;
  title: string;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  structure: number;
  pages: PageInfo[];
}

export interface ApiError {
  code: string;
  message: string;
  hint: string | null;
}

export interface ActionInfo {
  id: string;
  title: string;
  description: string;
  tool: boolean;
  shortcut: string | null;
}

export interface ModUi {
  id: string;
  name: string;
  disabled: boolean;
  actions: ActionInfo[];
  toolbar: { actionId: string; icon: string | null; tooltip: string }[];
  panels: { id: string; title: string; entry: string }[];
  /** The mod comes from a developer folder. */
  developer: boolean;
}

export type Permission = "document.read" | "document.edit" | "ui" | "files.pick";

/** One row of the mod manager. */
export interface ModRow {
  /** The mod ID, or the folder of a developer folder without a valid manifest. */
  key: string;
  id: string | null;
  name: string;
  version: string;
  author: string;
  description: string;
  source: "builtin" | "developer" | "installed";
  state: "on" | "off" | "stopped" | "failed";
  error: string | null;
  permissions: Permission[];
  folder: string | null;
}

/** A checked package that waits for the user's answer. */
export interface Offer {
  token: number;
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  permissions: Permission[];
  /** Permissions that the installed version does not have. */
  newPermissions: Permission[];
  previousVersion: string | null;
  sha256: string;
}

export interface Stroke {
  color: string;
  width: number;
  dash: number[];
}

export type Point2 = [number, number];

export type Shape =
  | { type: "rect"; page: PageId; bounds: Box2; ellipse: boolean; stroke: Stroke | null; fill: string | null; label: string }
  | { type: "line"; page: PageId; start: Point2; end: Point2; stroke: Stroke; startHead: string; endHead: string; label: string }
  | { type: "polyline"; page: PageId; points: Point2[]; closed: boolean; stroke: Stroke | null; fill: string | null; label: string }
  | { type: "text"; page: PageId; at: Point2; text: string; fontFamily: string; size: number; color: string; bold: boolean; italic: boolean; label: string }
  | { type: "image"; page: PageId; bounds: Box2; image: number; opacity: number; label: string }
  | { type: "caret"; page: PageId; at: Point2; height: number; color: string; label: string }
  | { type: "handle"; page: PageId; at: Point2; kind: string; label: string };

export interface TextRun {
  text: string;
  box: Box2;
  size: number;
  vertical: boolean;
}

export interface Hit {
  page: PageId;
  text: string;
  boxes: Box2[];
}

export interface OutlineItem {
  title: string;
  page: PageId | null;
  children: OutlineItem[];
}

export interface PointerReply {
  cursor: string;
  handled: boolean;
  overlay: Shape[];
}
