import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PageHeader } from "./PageHeader";

describe("PageHeader responsive layout", () => {
  it("stacks the title and full-width actions on narrow screens", () => {
    render(<PageHeader title="Client Management" actions={<button>Add client</button>} />);
    expect(screen.getByRole("heading", { name: "Client Management" }).parentElement.parentElement.className).toContain("flex-col");
    expect(screen.getByRole("button", { name: "Add client" }).parentElement.className).toContain("w-full");
  });
});
