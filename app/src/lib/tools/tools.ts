// The page tools offered in the Tools menu and on the start page. A tool is
// listed once it's built; the rest of M3 adds to this list.

import type { ComponentProps } from "svelte";
import type Icon from "../Icon.svelte";

export type ToolId =
  | "organize"
  | "extract"
  | "split"
  | "compress"
  | "export-images"
  | "protect"
  | "flatten"
  | "merge"
  | "images-to-pdf"
  | "html-to-pdf"
  | "repair";

export interface ToolInfo {
  id: ToolId;
  /** Translation key of its name. */
  label: string;
  icon: ComponentProps<typeof Icon>["name"];
  /** Works on the open document ("document") or makes a new one ("create"). */
  group: "document" | "create";
  /** Needs an open document (or asks for a file when there's none). */
  needsDocument: boolean;
  /** Changes or takes out pages, which the document's author may not allow. */
  needsAssemble: boolean;
}

export const TOOLS: ToolInfo[] = [
  {
    id: "organize",
    label: "organize-pages",
    icon: "pages",
    group: "document",
    needsDocument: true,
    needsAssemble: true,
  },
  {
    id: "extract",
    label: "extract-pages",
    icon: "extract",
    group: "document",
    needsDocument: true,
    needsAssemble: true,
  },
  {
    id: "split",
    label: "split-tool",
    icon: "split",
    group: "document",
    needsDocument: true,
    needsAssemble: true,
  },
  {
    id: "compress",
    label: "compress-tool",
    icon: "compress",
    group: "document",
    needsDocument: true,
    needsAssemble: false,
  },
  {
    id: "export-images",
    label: "export-tool",
    icon: "image",
    group: "document",
    needsDocument: true,
    needsAssemble: false,
  },
  {
    id: "protect",
    label: "protect-tool",
    icon: "lock",
    group: "document",
    needsDocument: true,
    needsAssemble: false,
  },
  {
    id: "flatten",
    label: "flatten-tool",
    icon: "flatten",
    group: "document",
    needsDocument: true,
    needsAssemble: false,
  },
  {
    id: "merge",
    label: "merge-tool",
    icon: "merge",
    group: "create",
    needsDocument: false,
    needsAssemble: false,
  },
  {
    id: "images-to-pdf",
    label: "images-tool",
    icon: "image",
    group: "create",
    needsDocument: false,
    needsAssemble: false,
  },
  {
    id: "html-to-pdf",
    label: "html-tool",
    icon: "globe",
    group: "create",
    needsDocument: false,
    needsAssemble: false,
  },
  {
    id: "repair",
    label: "repair-tool",
    icon: "wrench",
    group: "create",
    needsDocument: false,
    needsAssemble: false,
  },
];
