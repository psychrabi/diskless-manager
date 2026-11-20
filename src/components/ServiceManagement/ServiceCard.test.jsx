import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import ServiceCard from "./ServiceCard";

it("keeps service identity and running state visible without an optional display name", async () => {
  const onViewConfig = vi.fn();
  const { rerender } = render(<ServiceCard service={{ name: "dhcp", running: true }} onViewConfig={onViewConfig} />);
  expect(screen.getByText("DHCP")).toBeVisible();
  expect(screen.getByText("Running")).toBeVisible();
  expect(screen.getByRole("button", { name: "Restart" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Start", exact: true })).not.toBeInTheDocument();
  await userEvent.setup().click(screen.getByRole("button", { name: "View Config" }));
  expect(onViewConfig).toHaveBeenCalledWith("dhcp", "DHCP");
  rerender(<ServiceCard service={{ name: "dhcp", display_name: "DHCP Server", running: false }} />);
  expect(screen.getByText("DHCP Server")).toBeVisible();
  expect(screen.getByText("Stopped")).toBeVisible();
  expect(screen.getByRole("button", { name: "Start", exact: true })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Stop", exact: true })).not.toBeInTheDocument();
});
