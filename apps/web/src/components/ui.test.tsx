import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CategoryBadge, Field, Tip } from "./ui";
import { setPrefs } from "../lib/prefs";

describe("ui primitives", () => {
  it("connects labels, hints and errors to inputs", () => {
    render(<Field label="Title" hint="Short is fine" error="Please add a title.">{({ id, describedBy, invalid }) => <input id={id} aria-describedby={describedBy} aria-invalid={invalid} />}</Field>);
    const input = screen.getByLabelText("Title");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAccessibleDescription(/Short is fine.*Please add a title/);
    expect(screen.getByRole("alert")).toHaveTextContent("Please add a title.");
  });
  it("category badges carry a text label, not just color", () => {
    render(<CategoryBadge category="errands" />);
    expect(screen.getByText("Errands")).toBeInTheDocument();
  });
  it("tips can be dismissed and stay hidden; quiet mode hides them", async () => {
    setPrefs({ dismissedTips: [], quiet: false });
    const { rerender } = render(<Tip id="t1">Hello</Tip>);
    await userEvent.click(screen.getByRole("button", { name: "Dismiss tip" }));
    rerender(<Tip id="t1">Hello</Tip>);
    expect(screen.queryByText("Hello")).not.toBeInTheDocument();
    setPrefs({ quiet: true });
    render(<Tip id="t2">Quiet</Tip>);
    expect(screen.queryByText("Quiet")).not.toBeInTheDocument();
  });
});
