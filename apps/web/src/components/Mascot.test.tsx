import { render, screen } from "@testing-library/react";
import { Mascot } from "./Mascot";

describe("Mascot", () => {
  it("is decorative unless labelled", () => {
    const { container } = render(<Mascot />);
    expect(container.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
  });
  it("exposes a label when meaningful", () => {
    render(<Mascot pose="wave" label="Pim waving" />);
    expect(screen.getByRole("img", { name: "Pim waving" })).toBeInTheDocument();
  });
  it.each(["happy", "wave", "sleepy", "celebrate", "think"] as const)("renders the %s pose", (pose) => {
    const { container } = render(<Mascot pose={pose} animated={false} />);
    expect(container.querySelectorAll("path").length).toBeGreaterThan(4);
  });
});
