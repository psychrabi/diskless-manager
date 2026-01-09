import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import OperationConfirmDialog from "./OperationConfirmDialog";

// Mock the Modal component to avoid dialog issues in tests
vi.mock("@/components/ui", () => ({
  Modal: ({ isOpen, onClose, title, children }) => (
    isOpen ? (
      <div data-testid="modal" role="dialog">
        <h2>{title}</h2>
        {children}
        <button onClick={onClose}>Close</button>
      </div>
    ) : null
  ),
  Button: ({ onClick, children, disabled, variant }) => (
    <button onClick={onClick} disabled={disabled} data-variant={variant}>
      {children}
    </button>
  ),
}));

describe("OperationConfirmDialog", () => {
  const mockOnClose = vi.fn();
  const mockOnConfirm = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders dialog when open", () => {
    render(
      <OperationConfirmDialog
        isOpen={true}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Test Dialog"
        description="Test description"
      />
    );

    expect(screen.getByText("Test Dialog")).toBeInTheDocument();
    expect(screen.getByText("Test description")).toBeInTheDocument();
  });

  it("does not render when closed", () => {
    const { container } = render(
      <OperationConfirmDialog
        isOpen={false}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Test Dialog"
      />
    );

    expect(container.querySelector('[data-testid="modal"]')).not.toBeInTheDocument();
  });

  it("displays client name when provided", () => {
    render(
      <OperationConfirmDialog
        isOpen={true}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Confirm Operation"
        clientName="test-client"
      />
    );

    expect(screen.getByText("test-client")).toBeInTheDocument();
  });

  it("calls onConfirm when confirm button is clicked", async () => {
    const user = userEvent.setup();
    render(
      <OperationConfirmDialog
        isOpen={true}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Confirm"
      />
    );

    const confirmButton = screen.getByRole("button", { name: /confirm/i });
    await user.click(confirmButton);

    expect(mockOnConfirm).toHaveBeenCalled();
  });

  it("calls onClose when cancel button is clicked", async () => {
    const user = userEvent.setup();
    render(
      <OperationConfirmDialog
        isOpen={true}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Confirm"
      />
    );

    const cancelButton = screen.getByRole("button", { name: /cancel/i });
    await user.click(cancelButton);

    expect(mockOnClose).toHaveBeenCalled();
  });

  it("disables buttons when loading", () => {
    render(
      <OperationConfirmDialog
        isOpen={true}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Confirm"
        isLoading={true}
      />
    );

    const confirmButton = screen.getByRole("button", { name: /processing/i });
    const cancelButton = screen.getByRole("button", { name: /cancel/i });

    expect(confirmButton).toBeDisabled();
    expect(cancelButton).toBeDisabled();
  });

  it("displays warning variant by default", () => {
    render(
      <OperationConfirmDialog
        isOpen={true}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Confirm"
      />
    );

    const modal = screen.getByTestId("modal");
    expect(modal).toBeInTheDocument();
  });

  it("displays danger variant when specified", () => {
    render(
      <OperationConfirmDialog
        isOpen={true}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Confirm"
        variant="danger"
      />
    );

    const modal = screen.getByTestId("modal");
    expect(modal).toBeInTheDocument();
  });

  it("displays success variant when specified", () => {
    render(
      <OperationConfirmDialog
        isOpen={true}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Confirm"
        variant="success"
      />
    );

    const modal = screen.getByTestId("modal");
    expect(modal).toBeInTheDocument();
  });

  it("displays custom description", () => {
    const customDescription = "Are you sure you want to perform this action?";
    render(
      <OperationConfirmDialog
        isOpen={true}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Confirm"
        description={customDescription}
      />
    );

    expect(screen.getByText(customDescription)).toBeInTheDocument();
  });

  it("displays default description with operation type and client name", () => {
    render(
      <OperationConfirmDialog
        isOpen={true}
        onClose={mockOnClose}
        onConfirm={mockOnConfirm}
        title="Confirm"
        operationType="shutdown"
        clientName="my-client"
      />
    );

    expect(screen.getByText(/shutdown.*my-client/i)).toBeInTheDocument();
  });
});
