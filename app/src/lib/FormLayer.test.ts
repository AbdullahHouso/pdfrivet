// @vitest-environment happy-dom
// Component test: the form layer turns clicks and typing into field changes.

import { cleanup, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FormField } from "./bindings/FormField";

const field = (index: number, name: string, kind: FormField["field"], readOnly = false): FormField => ({
  index,
  name,
  readOnly,
  left: 0.1,
  top: 0.1 * index,
  right: 0.5,
  bottom: 0.1 * index + 0.05,
  field: kind,
});

const fields: FormField[] = [
  field(1, "name", { kind: "text", value: "", multiline: false, password: false }),
  field(2, "agree", { kind: "checkbox", checked: false }),
  field(3, "country", { kind: "choice", options: ["Saudi Arabia", "Egypt"], selected: 1 }),
  field(4, "locked", { kind: "text", value: "x", multiline: false, password: false }, true),
];

const changeField = vi.fn(async () => {});
vi.mock("./pdf", () => ({
  getFormFields: async () => fields,
  changeField: (...args: unknown[]) => changeField(...(args as [])),
  toRivetError: (e: unknown) => e,
}));

const { default: FormLayer } = await import("./FormLayer.svelte");

function setup() {
  const onchanged = vi.fn();
  render(FormLayer, { docId: 7, index: 0, rotation: 0, pageHeight: 800, revision: 0, onchanged });
  return { user: userEvent.setup(), onchanged };
}

describe("FormLayer", () => {
  beforeEach(() => changeField.mockClear());
  afterEach(() => cleanup());

  it("types into a text field and commits on Enter", async () => {
    const { user, onchanged } = setup();
    await user.click(await screen.findByRole("button", { name: "name" }));
    const box = screen.getByRole("textbox", { name: "name" });
    expect(document.activeElement).toBe(box);
    await user.type(box, "Rivet ريفت{Enter}");
    expect(changeField).toHaveBeenCalledWith(7, 0, 1, { kind: "text", value: "Rivet ريفت" });
    expect(onchanged).toHaveBeenCalled();
  });

  it("Escape cancels without changing the field", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: "name" }));
    await user.type(screen.getByRole("textbox", { name: "name" }), "oops{Escape}");
    expect(changeField).not.toHaveBeenCalled();
  });

  it("toggles a checkbox", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("checkbox", { name: "agree" }));
    expect(changeField).toHaveBeenCalledWith(7, 0, 2, { kind: "toggle" });
  });

  it("selects an option", async () => {
    const { user } = setup();
    await user.selectOptions(await screen.findByRole("combobox", { name: "country" }), "Saudi Arabia");
    expect(changeField).toHaveBeenCalledWith(7, 0, 3, { kind: "select", option: 0 });
  });

  it("does not offer read-only fields", async () => {
    setup();
    await screen.findByRole("button", { name: "name" });
    expect(screen.queryByRole("button", { name: "locked" })).toBeNull();
  });
});
