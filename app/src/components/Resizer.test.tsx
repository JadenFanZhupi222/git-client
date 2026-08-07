import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { Resizer, useResizableWidth } from "./Resizer";

function Harness() {
  const col = useResizableWidth("test.column", 320, 220, 640);
  return (
    <>
      <output>{col.w}</output>
      <Resizer
        value={col.w}
        min={col.min}
        max={col.max}
        label="Resize panel"
        onDown={col.onDown}
        onKeyDown={col.onKeyDown}
        onReset={col.reset}
      />
    </>
  );
}

function ClampHarness() {
  const col = useResizableWidth("test.clamped", 240, 320, 480);
  return <output>{col.w}</output>;
}

describe("Resizer", () => {
  beforeEach(() => localStorage.clear());

  it("supports keyboard resizing and resetting", () => {
    render(<Harness />);
    const separator = screen.getByRole("separator", { name: "Resize panel" });

    fireEvent.keyDown(separator, { key: "ArrowRight" });
    expect(screen.getByText("336")).toBeInTheDocument();

    fireEvent.keyDown(separator, { key: "ArrowRight", shiftKey: true });
    expect(screen.getByText("384")).toBeInTheDocument();

    fireEvent.keyDown(separator, { key: "Home" });
    expect(screen.getByText("320")).toBeInTheDocument();

    expect(separator).toHaveAttribute("aria-valuemin", "220");
    expect(separator).toHaveAttribute("aria-valuemax", "640");
  });

  it("clamps responsive defaults to a usable minimum", () => {
    render(<ClampHarness />);
    expect(screen.getByText("320")).toBeInTheDocument();
  });
});
