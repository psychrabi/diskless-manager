import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import RebootModal from "./RebootModal";
import * as api from "@/api/commands";

// Mock the API
vi.mock("@/api/commands", () => ({
  rebootClient: vi.fn(),
}));

// Mock the toast store
vi.mock("@/store/useToastStore", () => ({
  useToastStore: () => ({
    success: vi.fn(),
    error: vi.fn(),
  }),
}));

// Mock the UI components
vi.mock("@/components/ui", () => ({
  Modal: ({ isOpen, onClose, title, children }) => (
    isOpen ? (
      <div data-testid="modal" role="dialog">
        <h2>{title}</h2>
        {children}
      </div>
    ) : null
  ),
  Button: ({ onClick, children, disabled, type, variant }) => (
    <button onClick={onClick} disabled={disabled} type={type} data-variant={variant}>
      {children}
    </button>
  ),
}));

describe("RebootModal", () => {
  const mockClient = {
    id: 1,
    name: "test-client",
    status: "Online",
  };

  const mockOnClose = vi.fn();
  const mockOnSuccess = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders reboot modal when open", () => {
    render(
      <RebootModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    expect(screen.getByText("Reboot Client")).toBeInTheDocument();
    expect(screen.getByText("test-client")).toBeInTheDocument();
  });

  it("displays graceful reboot option by default", () => {
    render(
      <RebootModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const gracefulRadio = screen.getByDisplayValue("graceful");
    expect(gracefulRadio).toBeChecked();
  });

  it("allows switching to force reboot mode", async () => {
    const user = userEvent.setup();
    render(
      <RebootModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const forceRadio = screen.getByDisplayValue("force");
    await user.click(forceRadio);

    expect(forceRadio).toBeChecked();
  });

  it("allows scheduling reboot with delay", async () => {
    const user = userEvent.setup();
    render(
      <RebootModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const scheduleCheckbox = screen.getByRole("checkbox", {
      name: /schedule reboot/i,
    });
    await user.click(scheduleCheckbox);

    expect(scheduleCheckbox).toBeChecked();
  });

  it("calls rebootClient API with graceful mode", async () => {
    const user = userEvent.setup();
    api.rebootClient.mockResolvedValue({
      message: "Reboot command sent",
    });

    render(
      <RebootModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const submitButton = screen.getByRole("button", { name: /reboot/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(api.rebootClient).toHaveBeenCalledWith(mockClient.id, {
        force: false,
        delay_minutes: null,
      });
    });
  });

  it("calls rebootClient API with force mode", async () => {
    const user = userEvent.setup();
    api.rebootClient.mockResolvedValue({
      message: "Reboot command sent",
    });

    render(
      <RebootModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const forceRadio = screen.getByDisplayValue("force");
    await user.click(forceRadio);

    const submitButton = screen.getByRole("button", { name: /reboot/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(api.rebootClient).toHaveBeenCalledWith(mockClient.id, {
        force: true,
        delay_minutes: null,
      });
    });
  });

  it("calls rebootClient API with delay when scheduled", async () => {
    const user = userEvent.setup();
    api.rebootClient.mockResolvedValue({
      message: "Reboot scheduled",
    });

    render(
      <RebootModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const scheduleCheckbox = screen.getByRole("checkbox", {
      name: /schedule reboot/i,
    });
    await user.click(scheduleCheckbox);

    const delayInput = screen.getByDisplayValue("0");
    await user.clear(delayInput);
    await user.type(delayInput, "15");

    const submitButton = screen.getByRole("button", { name: /reboot/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(api.rebootClient).toHaveBeenCalledWith(mockClient.id, {
        force: false,
        delay_minutes: 15,
      });
    });
  });

  it("closes modal on cancel", async () => {
    const user = userEvent.setup();
    render(
      <RebootModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const cancelButton = screen.getByRole("button", { name: /cancel/i });
    await user.click(cancelButton);

    expect(mockOnClose).toHaveBeenCalled();
  });

  it("calls onSuccess callback after successful reboot", async () => {
    const user = userEvent.setup();
    api.rebootClient.mockResolvedValue({
      message: "Reboot command sent",
    });

    render(
      <RebootModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const submitButton = screen.getByRole("button", { name: /reboot/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(mockOnSuccess).toHaveBeenCalled();
    });
  });
});
