import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { executeSshCommand } from "@/api/modules/ssh";
import SshTester from "./SshTester";

vi.mock("@/api/modules/ssh", () => ({
  executeSshCommand: vi.fn().mockResolvedValue({ success: true, message: "Done", duration_ms: 1 }),
  testSshConnection: vi.fn(), getWindowsSystemInfo: vi.fn(),
}));

it("explains missing connection details and reuses credentials only when requested", async () => {
  const user = userEvent.setup();
  render(<SshTester />);
  expect(screen.getByRole("button", { name: "Test Connection" })).toBeDisabled();
  expect(screen.getByText(/Enter the host address/)).toBeVisible();
  const hosts = screen.getAllByLabelText("Host/IP Address");
  const passwords = screen.getAllByLabelText("Password");
  expect(passwords[0]).toHaveAccessibleDescription("Leave blank to use SSH key authentication.");
  await user.type(hosts[0], "192.0.2.20");
  await user.type(passwords[0], "synthetic-password");
  expect(hosts[1]).toHaveValue("");
  await user.click(screen.getByRole("button", { name: "Reuse credentials" }));
  expect(hosts[1]).toHaveValue("192.0.2.20");
  expect(passwords[1]).toHaveValue("synthetic-password");
  await user.click(screen.getByRole("button", { name: "Execute Command" }));
  expect(executeSshCommand).toHaveBeenCalledWith("192.0.2.20", "Administrator", 'echo "Hello from Windows SSH"', "synthetic-password");
});
