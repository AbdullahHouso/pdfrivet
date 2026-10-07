// Native printing: talks to the Rust side (rivet-core/src/print).

import { invoke } from "@tauri-apps/api/core";
import type { Orientation } from "./bindings/Orientation";
import type { Placement } from "./bindings/Placement";
import type { PrinterInfo } from "./bindings/PrinterInfo";
import type { PrintSettings } from "./bindings/PrintSettings";
import type { Scaling } from "./bindings/Scaling";
import { parsePageRange } from "./pageRange";

/** What the printer driver's own settings window chose (Windows). */
export interface PrinterProperties {
  devmode: number[];
  copies: number;
  duplex: PrintSettings["duplex"];
  grayscale: boolean;
  paper: string | null;
  landscape: boolean;
}

export function listPrinters(): Promise<PrinterInfo[]> {
  return invoke("list_printers");
}

export function printPlacement(
  page: { width: number; height: number },
  paperMm: { width: number; height: number },
  scaling: Scaling,
  orientation: Orientation,
): Promise<Placement> {
  return invoke("print_placement", {
    pageWidth: page.width,
    pageHeight: page.height,
    paperWidthMm: paperMm.width,
    paperHeightMm: paperMm.height,
    scaling,
    orientation,
  });
}

/** Opens the driver's settings window; resolves to null if cancelled or unavailable. */
export function printerProperties(printer: string, current: number[] | null): Promise<PrinterProperties | null> {
  return invoke("printer_properties", { printer, current });
}

export function printDocument(docId: number, settings: PrintSettings, printerSettings: number[] | null): Promise<void> {
  return invoke("print_document", { docId, settings, printerSettings });
}

export type PageChoice = "all" | "current" | "custom";
export type Subset = "all" | "odd" | "even";

/**
 * The pages to print (0-based, in print order) from the dialog's choices,
 * or null if the custom range is invalid. Odd/even count page numbers (1-based).
 */
export function choosePages(
  choice: PageChoice,
  current: number,
  pageCount: number,
  rangeText: string,
  subset: Subset,
  reverse: boolean,
): number[] | null {
  let pages: number[] | null;
  if (choice === "all") pages = Array.from({ length: pageCount }, (_, i) => i);
  else if (choice === "current") pages = [current];
  else pages = parsePageRange(rangeText, pageCount);
  if (!pages) return null;
  if (subset === "odd") pages = pages.filter((p) => (p + 1) % 2 === 1);
  if (subset === "even") pages = pages.filter((p) => (p + 1) % 2 === 0);
  return reverse ? [...pages].reverse() : pages;
}
