import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import LinuxImageBuilderModal from "./LinuxImageBuilderModal";

const mocks = vi.hoisted(() => ({
  listMedia: vi.fn(),
  preflight: vi.fn(),
  build: vi.fn(),
  success: vi.fn(),
  error: vi.fn(),
}));

vi.mock("@/api/modules/installerMedia", () => ({ listInstallerMedia: mocks.listMedia }));
vi.mock("@/api/modules/linuxImageBuilder", () => ({
  preflightLinuxImageBuild: mocks.preflight,
  buildLinuxImage: mocks.build,
}));
vi.mock("@/store/useToastStore", () => ({
  useToastStore: () => ({ success: mocks.success, error: mocks.error }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listMedia.mockResolvedValue([{
    id: "ubuntu-iso",
    display_name: "Ubuntu Desktop",
    media_type: "linux_iso",
    validation_state: "valid",
  }]);
  mocks.preflight.mockResolvedValue({ distro: "ubuntu", minimum_size_gb: 20, target_size_gb: 50 });
  mocks.build.mockResolvedValue({ name: "tank/images/lab-linux" });
});

it("builds with the selected first-boot account and sudo setting", async () => {
  const onClose = vi.fn();
  render(<LinuxImageBuilderModal isOpen onClose={onClose} />);

  fireEvent.change(await screen.findByLabelText("Image name"), { target: { value: "lab-linux" } });
  fireEvent.change(screen.getByLabelText("First-boot username"), { target: { value: "labadmin" } });
  fireEvent.change(screen.getByLabelText("First-boot password"), { target: { value: "a-private-password" } });
  const sudoOption = screen.getByRole("checkbox", { name: "Grant this account sudo access" });
  expect(sudoOption).toBeChecked();
  fireEvent.click(sudoOption);
  expect(sudoOption).not.toBeChecked();
  fireEvent.click(sudoOption);

  fireEvent.click(screen.getByRole("button", { name: "Check ISO" }));
  expect(await screen.findByText(/ISO preflight passed/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("checkbox", { name: /Create a new 50 GiB ZFS image/ }));
  fireEvent.click(screen.getByRole("button", { name: "Build Image" }));

  await waitFor(() => expect(mocks.build).toHaveBeenCalledWith({
    media_id: "ubuntu-iso",
    name: "lab-linux",
    size_gb: 50,
    username: "labadmin",
    password: "a-private-password",
    sudo_user: true,
    confirm_create: true,
  }));
  expect(onClose).toHaveBeenCalledOnce();
});
