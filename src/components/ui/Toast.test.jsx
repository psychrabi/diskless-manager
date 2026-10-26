import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import Toast from "./Toast";

it.each([
  ["success", "status"],
  ["error", "alert"],
])("announces %s notifications as a %s", (type, role) => {
  render(<Toast toast={{ id: "toast-1", type, title: "Operation finished" }} />);

  expect(screen.getByRole(role)).toHaveAttribute("aria-atomic", "true");
});
