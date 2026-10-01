import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import PrivilegeManagementForm from "./PrivilegeManagementForm";
import AuthorizeStep from "../Setup/AuthorizeStep";

const api = vi.hoisted(() => ({
  checkPrivilegedAccess: vi.fn(), setupPrivilegedAccess: vi.fn(),
}));
vi.mock("@/api/modules/system", () => api);

beforeEach(() => {
  vi.clearAllMocks();
  api.checkPrivilegedAccess.mockResolvedValue({ authorized: false });
});

it("disables authorization when the existing grant is confirmed", async () => {
  api.checkPrivilegedAccess.mockResolvedValue({ authorized: true });
  render(<PrivilegeManagementForm />);
  expect(await screen.findByRole("button", { name: "Already Authorized" })).toBeDisabled();
  expect(api.setupPrivilegedAccess).not.toHaveBeenCalled();
});

it.each(["settings", "wizard"])("keeps terminal setup instructions visible after a %s authorization failure", async (view) => {
  const onAuthorized = vi.fn();
  const message = "No authentication agent found. Run '/opt/diskless-manager' authorize in a terminal on the server.";
  api.setupPrivilegedAccess.mockRejectedValue(new Error(message));
  render(view === "settings" ? <PrivilegeManagementForm /> : <AuthorizeStep onAuthorized={onAuthorized} />);
  const button = await screen.findByRole("button", { name: "Authorize Application", exact: true });
  await waitFor(() => expect(button).toBeEnabled());
  await userEvent.setup().click(button);
  expect(await screen.findByText(message)).toBeVisible();
  expect(onAuthorized).not.toHaveBeenCalled();
  expect(button).toBeEnabled();
});
