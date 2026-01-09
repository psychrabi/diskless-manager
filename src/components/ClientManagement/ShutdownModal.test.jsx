import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ShutdownModal from "./ShutdownModal";
import * as api from "@/api/commands";

// Mock the API
vi.mock("@/api/commands", () => ({
  shutdownClient: vi.fn(),
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

describe("ShutdownModal", () => {
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

  it("renders shutdown modal when open", () => {
    render(
      <ShutdownModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    expect(screen.getByText("Shutdown Client")).toBeInTheDocument();
    expect(screen.getByText("test-client")).toBeInTheDocument();
  });

  it("does not render when closed", () => {
    const { container } = render(
      <ShutdownModal
        client={mockClient}
        isOpen={false}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const modal = container.querySelector('[data-testid="modal"]');
    expect(modal).not.toBeInTheDocument();
  });

  it("displays graceful shutdown option by default", () => {
    render(
      <ShutdownModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const gracefulRadio = screen.getByDisplayValue("graceful");
    expect(gracefulRadio).toBeChecked();
  });

  it("allows switching to force shutdown mode", async () => {
    const user = userEvent.setup();
    render(
      <ShutdownModal
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

  it("allows scheduling shutdown with delay", async () => {
    const user = userEvent.setup();
    render(
      <ShutdownModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const scheduleCheckbox = screen.getByRole("checkbox", {
      name: /schedule shutdown/i,
    });
    await user.click(scheduleCheckbox);

    expect(scheduleCheckbox).toBeChecked();
  });

  it("calls shutdownClient API with graceful mode", async () => {
    const user = userEvent.setup();
    api.shutdownClient.mockResolvedValue({ message: "Shutdown initiated" });

    render(
      <ShutdownModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const submitButton = screen.getByRole("button", { name: /shutdown/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(api.shutdownClient).toHaveBeenCalledWith(1, {
        force: false,
        delay_minutes: null,
      });
    });
  });

  it("calls shutdownClient API with force mode", async () => {
    const user = userEvent.setup();
    api.shutdownClient.mockResolvedValue({ message: "Shutdown initiated" });

    render(
      <ShutdownModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const forceRadio = screen.getByDisplayValue("force");
    await user.click(forceRadio);

    const submitButton = screen.getByRole("button", { name: /shutdown/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(api.shutdownClient).toHaveBeenCalledWith(1, {
        force: true,
        delay_minutes: null,
      });
    });
  });

  it("calls shutdownClient API with delay when scheduled", async () => {
    const user = userEvent.setup();
    api.shutdownClient.mockResolvedValue({ message: "Shutdown scheduled" });

    render(
      <ShutdownModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const scheduleCheckbox = screen.getByRole("checkbox", {
      name: /schedule shutdown/i,
    });
    await user.click(scheduleCheckbox);

    const delayInput = screen.getByDisplayValue("0");
    await user.clear(delayInput);
    await user.type(delayInput, "10");

    const submitButton = screen.getByRole("button", { name: /shutdown/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(api.shutdownClient).toHaveBeenCalledWith(1, {
        force: false,
        delay_minutes: 10,
      });
    });
  });

  it("closes modal on cancel", async () => {
    const user = userEvent.setup();
    render(
      <ShutdownModal
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

  it("calls onSuccess callback after successful shutdown", async () => {
    const user = userEvent.setup();
    api.shutdownClient.mockResolvedValue({ message: "Shutdown initiated" });

    render(
      <ShutdownModal
        client={mockClient}
        isOpen={true}
        onClose={mockOnClose}
        onSuccess={mockOnSuccess}
      />
    );

    const submitButton = screen.getByRole("button", { name: /shutdown/i });
    await user.click(submitButton);

    await waitFor(() => {
      expect(mockOnSuccess).toHaveBeenCalled();
    });
  });
});
