import { fireEvent, render, screen } from "@testing-library/react";
import { LazyBoundary } from "./LazyBoundary";

function BrokenView(): never {
  throw new Error("chunk unavailable");
}

describe("LazyBoundary", () => {
  it("shows a recoverable error action when a lazy view fails", () => {
    const onRetry = vi.fn();
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);

    render(
      <LazyBoundary
        loading={<span>Loading</span>}
        message="Unable to load this view."
        retryLabel="Reload"
        onRetry={onRetry}
      >
        <BrokenView />
      </LazyBoundary>,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Unable to load this view.",
    );
    fireEvent.click(screen.getByRole("button", { name: "Reload" }));
    expect(onRetry).toHaveBeenCalledOnce();

    consoleError.mockRestore();
  });
});
