// All UI strings. Mod titles and messages come from the mods and are not here.

export const S = {
  appName: "T-Rustable",
  menu: {
    file: "File",
    edit: "Edit",
    view: "View",
    tools: "Tools",
    open: "Open...",
    save: "Save",
    saveAs: "Save as...",
    close: "Close tab",
    undo: "Undo",
    redo: "Redo",
    zoomIn: "Zoom in",
    zoomOut: "Zoom out",
    zoomReset: "Actual size",
    fitWidth: "Fit width",
    palette: "Command palette",
    theme: "Change theme",
    noTool: "Select text (no tool)",
  },
  tabs: {
    label: "Open documents",
    close: (title: string) => `Close ${title}`,
    unsaved: "Unsaved changes",
  },
  status: {
    noDocument: "No document open. Press Ctrl+O to open a PDF.",
    page: (n: number, total: number) => `Page ${n} of ${total}`,
    zoom: (percent: number) => `${percent}%`,
    tool: (name: string) => `Tool: ${name}`,
    noTool: "Tool: select text",
  },
  viewer: {
    label: "Document pages",
    page: (label: string) => `Page ${label}`,
    overlay: "Tool preview",
  },
  dialog: {
    yes: "Yes",
    no: "No",
    ok: "OK",
    cancel: "Cancel",
    unsavedTitle: "Unsaved changes",
    unsavedMessage: (title: string) => `${title} has unsaved changes. Close it and lose the changes?`,
    signedTitle: "Signed document",
    signedMessage:
      "This document has digital signatures. Saving rewrites the file, so the signatures become invalid. Save anyway?",
    passwordTitle: "Password",
    passwordMessage: (name: string) => `${name} needs a password.`,
  },
  errors: {
    title: "Error",
  },
} as const;
