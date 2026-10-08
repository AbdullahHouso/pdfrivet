// Signatures saved for reuse, kept in their own file (signatures.json in the
// app's config folder) on this computer only, like everything else.

import { load, type Store } from "@tauri-apps/plugin-store";
import type { Color } from "./bindings/Color";

/** A signature drawn with the mouse, pen or finger: strokes in px, from (0, 0). */
export interface InkSignature {
  id: string;
  kind: "ink";
  strokes: { x: number; y: number }[][];
  /** Size of the strokes' box, in px. */
  width: number;
  height: number;
  color: Color;
  /** Stroke width in px (same units as the strokes). */
  lineWidth: number;
}

/** A signature from a photo or scan, its background removed. */
export interface ImageSignature {
  id: string;
  kind: "image";
  /** PNG, as a data URL. */
  png: string;
  width: number;
  height: number;
}

export type Signature = InkSignature | ImageSignature;

let store: Store | null = null;
let list = $state<Signature[]>([]);
let loaded: Promise<void> | null = null;

async function save() {
  try {
    await store?.set("signatures", list);
    await store?.save();
  } catch (e) {
    console.warn("[signatures] could not save", e);
  }
}

export const signatures = {
  /** Loads the saved signatures (once). */
  load(): Promise<void> {
    loaded ??= (async () => {
      try {
        store = await load("signatures.json", { autoSave: false, defaults: {} });
        list = (await store.get<Signature[]>("signatures")) ?? [];
        // Other windows may add or delete signatures too.
        await store.onKeyChange<Signature[]>("signatures", (value) => {
          list = value ?? [];
        });
      } catch (e) {
        console.warn("[signatures] none loaded", e);
      }
    })();
    return loaded;
  },
  get list() {
    return list;
  },
  add(signature: Signature) {
    list = [...list, signature];
    save();
  },
  remove(id: string) {
    list = list.filter((s) => s.id !== id);
    save();
  },
};

export function newSignatureId(): string {
  return `sig-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

/** The pixels of an image signature (decoded from its PNG). */
export async function signaturePixels(sig: ImageSignature): Promise<ImageData> {
  const image = new Image();
  image.src = sig.png;
  await image.decode();
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth;
  canvas.height = image.naturalHeight;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no canvas");
  ctx.drawImage(image, 0, 0);
  return ctx.getImageData(0, 0, canvas.width, canvas.height);
}
