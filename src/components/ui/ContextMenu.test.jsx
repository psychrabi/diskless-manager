import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { ContextMenu } from "./ContextMenu";
import { AuthContext } from "@/contexts/auth";

const actions = { edit: vi.fn(), inventoryBoot: vi.fn(), delete: vi.fn(), startSuper: vi.fn(), saveSuper: vi.fn(), discardSuper: vi.fn() };

function show(client, { role = "admin", onExitLegacySuper = vi.fn(), onClose = vi.fn(), superStatus, superMedia = [], superLoading = false, targetImage } = {}) {
  render(
    <AuthContext.Provider value={{ user: { role } }}>
      <ContextMenu
        isOpen
        xPos={10}
        yPos={10}
        targetClient={client}
        onClose={onClose}
        actions={actions}
        onExitLegacySuper={onExitLegacySuper}
        superStatus={superStatus}
        superMedia={superMedia}
        superLoading={superLoading}
        targetImage={targetImage}
      />
    </AuthContext.Provider>,
  );
  return { onExitLegacySuper, onClose };
}

it("offers an inventory boot for pending clients", () => {
  show({ name: "PC009", ip: "192.0.2.9", mac: "02:00:00:00:00:09", status: "Offline", master: "pending" });
  expect(screen.getByRole("button", { name: "Request Inventory Boot" })).toBeInTheDocument();
});

it("hides the inventory boot for provisioned clients", () => {
  show({ name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Offline", master: "tank/image/windows" });
  expect(screen.queryByRole("button", { name: "Request Inventory Boot" })).not.toBeInTheDocument();
});

// A client assigned an image directly, with no snapshot clone, is the super
// client even when a legacy or imported row never stored the mode flag.
it("offers the normal Super session when installer media is available", () => {
  const client = { id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Offline", master: "tank/image/windows", snapshot: "tank/image/windows@clean", pxe_mode: "uefi" };
  show(client, {
    targetImage: { os_type: "windows", status: "ready" },
    superMedia: [{ media_type: "winpe", validation_state: "valid", is_default: true }],
  });
  const start = screen.getByRole("button", { name: "Start Super session" });
  expect(start).toBeEnabled();
  fireEvent.click(start);
  expect(actions.startSuper).toHaveBeenCalledWith(client, false, undefined);
});

it("enables normal Super without a recorded ISO boot result", () => {
  show({ id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Offline", master: "tank/image/windows", snapshot: "tank/image/windows@clean", pxe_mode: "uefi" }, {
    targetImage: { os_type: "windows", status: "ready" },
    superMedia: [{ media_type: "winpe", validation_state: "valid", is_default: true, uefi_verified: true }],
  });
  const start = screen.getByRole("button", { name: "Start Super session" });
  expect(start).toBeEnabled();
  fireEvent.click(start);
  expect(actions.startSuper).toHaveBeenCalledWith(expect.objectContaining({ id: "c1" }), false, undefined);
});

it("starts Super with the global default ISO regardless of the image OS", () => {
  show({ id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Offline", master: "tank/image/windows", snapshot: "tank/image/windows@clean", pxe_mode: "uefi" }, {
    targetImage: { os_type: "windows", status: "ready" },
    superMedia: [{ media_type: "linux_iso", validation_state: "valid", is_default: true, uefi_verified: true }],
  });
  const start = screen.getByRole("button", { name: "Start Super session" });
  expect(start).toBeEnabled();
  fireEvent.click(start);
  expect(actions.startSuper).toHaveBeenCalledWith(expect.objectContaining({ id: "c1" }), false, undefined);
});

it("allows a ready direct image assignment without a snapshot to start Super", () => {
  show({ id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Offline", master: "tank/image/windows", pxe_mode: "uefi" }, {
    superStatus: { mode: "normal", legacy: false, session: null },
    targetImage: { os_type: "windows", status: "ready" },
    superMedia: [{ media_type: "winpe", validation_state: "valid", is_default: true, uefi_verified: true }],
  });
  expect(screen.getByRole("button", { name: "Start Super session" })).toBeEnabled();
});

it("shows session state and prevents saving a verification-only session while retaining game-disk preference", () => {
  const client = { id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Offline", master: "tank/image/windows", snapshot: "tank/image/windows@clean", use_game_disk: true };
  show(client, { superStatus: { session: { state: "active", verification_only: true } } });
  expect(screen.getByText("Verification only · changes are disposable")).toBeInTheDocument();
  expect(screen.getByText("Use game disk stays enabled; its LUN is temporarily unmapped.")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save unavailable (verification only)" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Discard Super Changes" })).toBeEnabled();
});

it("disables finishing actions while a Super session needs recovery", () => {
  show({ id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Offline", master: "tank/image/windows" }, {
    superStatus: { session: { state: "recovery_required", verification_only: false } },
  });
  expect(screen.getByText("Super session: Recovery required")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save Super Changes" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Discard Super Changes" })).toBeDisabled();
});

it("treats the API's lowercase online status as online for Super controls", () => {
  show({ id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "online", master: "tank/image/windows", snapshot: "tank/image/windows@clean", pxe_mode: "uefi" }, {
    targetImage: { os_type: "windows", status: "ready" },
    superMedia: [{ media_type: "winpe", validation_state: "valid", is_default: true, uefi_verified: true }],
  });
  expect(screen.queryByRole("button", { name: "Start Super session" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Start verification-only session" })).not.toBeInTheDocument();

  cleanup();
  show({ id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "online", master: "tank/image/windows" }, {
    superStatus: { session: { state: "active", verification_only: false } },
  });
  expect(screen.getByRole("button", { name: "Save Super Changes" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Discard Super Changes" })).toBeDisabled();
});

it("keeps Save and Discard disabled while the client is online", () => {
  show({ id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Online", master: "tank/image/windows" }, {
    superStatus: { session: { state: "active", verification_only: false } },
  });
  expect(screen.getByRole("button", { name: "Save Super Changes" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Discard Super Changes" })).toBeDisabled();
});

it("blocks controls for storage recovery but ignores boot-failure recovery rows", () => {
  const client = { id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Offline", master: "tank/image/windows", snapshot: "tank/image/windows@clean", pxe_mode: "uefi" };
  show(client, {
    superStatus: { mode: "normal", legacy: false, session: null, storage_recovery: true },
    targetImage: { os_type: "windows", status: "ready" },
    superMedia: [{ media_type: "winpe", validation_state: "valid", is_default: true, uefi_verified: true }],
  });
  expect(screen.getByRole("button", { name: "Super session unavailable" })).toBeDisabled();
  expect(screen.getByText(/storage recovery is pending/i)).toBeInTheDocument();
});

it("explains that a matching ISO exists but is not the default", () => {
  show({ id: "c1", name: "PC001", ip: "192.0.2.1", mac: "02:00:00:00:00:01", status: "Offline", master: "tank/image/linux", snapshot: "tank/image/linux@clean", pxe_mode: "uefi" }, {
    superStatus: { mode: "normal", legacy: false, session: null },
    targetImage: { os_type: "linux", status: "ready" },
    superMedia: [{ media_type: "linux_iso", validation_state: "valid", is_default: false }],
  });
  expect(screen.getByRole("button", { name: "Super session unavailable" })).toBeDisabled();
  expect(screen.getByText(/none is marked default/i)).toBeInTheDocument();
});

it("shows legacy exit only to admins and invokes its handler", () => {
  const client = {
    name: "PC002", ip: "192.0.2.2", mac: "02:00:00:00:00:02", status: "Offline",
    master: "tank/image/windows", legacy_super_state: "legacy",
  };
  const admin = show(client);
  const exitButton = screen.getByRole("button", { name: "Exit Legacy Super" });
  fireEvent.click(exitButton);
  expect(admin.onExitLegacySuper).toHaveBeenCalledWith(client);
  expect(admin.onClose).toHaveBeenCalled();

  cleanup();
  show({ ...client, name: "PC003" }, { role: "user" });
  expect(screen.queryByRole("button", { name: "Exit Legacy Super" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Save Super Changes" })).not.toBeInTheDocument();
});
