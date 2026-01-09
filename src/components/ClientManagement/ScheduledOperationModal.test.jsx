import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ScheduledOperationModal from "./ScheduledOperationModal";
import * as api from "@/api/commands";

// Mock the API
vi.mock("@/api/commands", () => ({
  shutdownClient: vi.fn(),
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

describe("ScheduledOperationModal", () => {
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

  it("renders scheduled operation modal when open", () => {
    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    expect(screen.getByText("Schedule Operation")).toBeInTheDocument();
    expect(screen.getByText("test-client")).toBeInTheDocument();
  });

  it("displays shutdown operation by default", () => {
    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const shutdownRadio = screen.getByDisplayValue("shutdown");
    expect(shutdownRadio).toBeChecked();
  });

  it("allows switching to reboot operation", async () => {
    const user = userEvent.setup();
    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const rebootRadio = screen.getByDisplayValue("reboot");
    await user.click(rebootRadio);

    expect(rebootRadio).toBeChecked();
  });

  it("displays graceful mode by default", () => {
    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const gracefulRadio = screen.getByDisplayValue("graceful");
    expect(gracefulRadio).toBeChecked();
  });

  it("allows switching to force mode", async () => {
    const user = userEvent.setup();
    render(
      <ScheduledOperationModal
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

  it("has default delay of 5 minutes", () => {
    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const delayInput = screen.getByDisplayValue("5");
    expect(delayInput).toHaveValue(5);
  });

  it("allows changing delay value", async () => {
    const user = userEvent.setup();
    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const delayInput = screen.getByDisplayValue("5");
    await user.click(delayInput);
    await user.keyboard("{Control>}a{/Control}");
    await user.type(delayInput, "30");

    expect(delayInput).toHaveValue(30);
  });

  it("calls shutdownClient API with scheduled shutdown", async () => {
    const user = userEvent.setup();
    api.shutdownClient.mockResolvedValue({
      message: "Shutdown scheduled",
    });

    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const delayInput = screen.getByDisplayValue("5");
    await user.click(delayInput);
    await user.keyboard("{Control>}a{/Control}");
    await user.type(delayInput, "10");

    const submitButton = screen.getByRole("button", { name: /schedule/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(api.shutdownClient).toHaveBeenCalled();
    });
  });

  it("calls rebootClient API with scheduled reboot", async () => {
    const user = userEvent.setup();
    api.rebootClient.mockResolvedValue({
      message: "Reboot scheduled",
    });

    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const rebootRadio = screen.getByDisplayValue("reboot");
    await user.click(rebootRadio);

    const delayInput = screen.getByDisplayValue("5");
    await user.click(delayInput);
    await user.keyboard("{Control>}a{/Control}");
    await user.type(delayInput, "20");

    const submitButton = screen.getByRole("button", { name: /schedule/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(api.rebootClient).toHaveBeenCalled();
    });
  });

  it("calls API with force mode when selected", async () => {
    const user = userEvent.setup();
    api.shutdownClient.mockResolvedValue({
      message: "Shutdown scheduled",
    });

    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const forceRadio = screen.getByDisplayValue("force");
    await user.click(forceRadio);

    const submitButton = screen.getByRole("button", { name: /schedule/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(api.shutdownClient).toHaveBeenCalled();
    });
  });

  it("closes modal on cancel", async () => {
    const user = userEvent.setup();
    render(
      <ScheduledOperationModal
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

  it("calls onSuccess callback after successful scheduling", async () => {
    const user = userEvent.setup();
    api.shutdownClient.mockResolvedValue({
      message: "Shutdown scheduled",
    });

    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const submitButton = screen.getByRole("button", { name: /schedule/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(mockOnSuccess).toHaveBeenCalled();
    });
  });

  it("disables schedule button when delay is less than 1", async () => {
    const user = userEvent.setup();
    render(
      <ScheduledOperationModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const delayInput = screen.getByDisplayValue("5");
    await user.click(delayInput);
    await user.keyboard("{Control>}a{/Control}");
    await user.type(delayInput, "0");

    const submitButtons = screen.getAllByRole("button", { name: /schedule/i });
    if (submitButtons.length > 0) {
      expect(submitButtons[0]).toBeDisabled();
    }
  });
});
