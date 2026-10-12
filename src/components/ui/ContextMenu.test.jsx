import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { ContextMenu } from "./ContextMenu";

const actions = { edit: vi.fn(), inventoryBoot: vi.fn(), delete: vi.fn() };

function show(client) {
  render(
    <ContextMenu
      isOpen
      xPos={10}
      yPos={10}
      targetClient={client}
      onClose={() => {}}
      actions={actions}
    />,
  );
}

it("offers an inventory boot for pending clients", () => {
  show({ name: "PC009", ip: "192.0.2.9", mac: "02:00:00:00:00:09", status: "Offline", master: "pending" });
  expect(screen.getByRole("button", { name: "Request Inventory Boot" })).toBeInTheDocument();
});

it("hides the inventory boot for provisioned clients", () => {
  show({ name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Offline", master: "tank/image/windows" });
  expect(screen.queryByRole("button", { name: "Request Inventory Boot" })).not.toBeInTheDocument();
});
