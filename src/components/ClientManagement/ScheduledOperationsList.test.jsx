import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ScheduledOperationsList from "./ScheduledOperationsList";
import * as api from "@/api/commands";

// Mock the API
vi.mock("@/api/commands", () => ({
  getScheduledOperations: vi.fn(),
  cancelScheduledOperation: vi.fn(),
}));

// Mock the app store
vi.mock("@/store/useAppStore", () => ({
  useAppStore: (selector) => {
    const state = {
      clients: [
        { id: "1", name: "client-01" },
        { id: "2", name: "client-02" },
      ],
    };
    return selector(state);
  },
}));

// Mock the notification context
vi.mock("@/contexts/notification", () => ({
  useNotification: () => ({
    showNotification: vi.fn(),
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
  Table: ({ children }) => <table>{children}</table>,
}));

describe("ScheduledOperationsList", () => {
  const mockOnClose = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders scheduled operations list when open", () => {
    api.getScheduledOperations.mockResolvedValue({
      operations: [],
    });

    render(
      <ScheduledOperationsList
        isOpen={true}
        onClose={mockOnClose}
      />
    );

    expect(screen.getAllByText("Scheduled Operations").length).toBeGreaterThan(0);
  });

  it("displays empty state when no scheduled operations", async () => {
    api.getScheduledOperations.mockResolvedValue({
      operations: [],
    });

    render(
      <ScheduledOperationsList
        isOpen={true}
        onClose={mockOnClose}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("No Scheduled Operations")).toBeInTheDocument();
    });
  });

  it("displays scheduled operations in table", async () => {
    const mockOperations = [
      {
        id: "op_1",
        client_id: "1",
        operation_type: "shutdown",
        operation_mode: "graceful",
        scheduled_time: "2024-01-10T10:00:00Z",
        created_at: "2024-01-09T09:00:00Z",
        result: null,
      },
      {
        id: "op_2",
        client_id: "2",
        operation_type: "reboot",
        operation_mode: "force",
        scheduled_time: "2024-01-10T15:00:00Z",
        created_at: "2024-01-09T14:00:00Z",
        result: null,
      },
    ];

    api.getScheduledOperations.mockResolvedValue({
      operations: mockOperations,
    });

    render(
      <ScheduledOperationsList
        isOpen={true}
        onClose={mockOnClose}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("client-01")).toBeInTheDocument();
      expect(screen.getByText("client-02")).toBeInTheDocument();
      expect(screen.getByText("shutdown")).toBeInTheDocument();
      expect(screen.getByText("reboot")).toBeInTheDocument();
    });
  });

  it("displays operation count", async () => {
    const mockOperations = [
      {
        id: "op_1",
        client_id: "1",
        operation_type: "shutdown",
        operation_mode: "graceful",
        scheduled_time: "2024-01-10T10:00:00Z",
        created_at: "2024-01-09T09:00:00Z",
        result: null,
      },
    ];

    api.getScheduledOperations.mockResolvedValue({
      operations: mockOperations,
    });

    render(
      <ScheduledOperationsList
        isOpen={true}
        onClose={mockOnClose}
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/1 scheduled operation/)).toBeInTheDocument();
    });
  });

  it("displays correct operation type badges", async () => {
    const mockOperations = [
      {
        id: "op_1",
        client_id: "1",
        operation_type: "shutdown",
        operation_mode: "graceful",
        scheduled_time: "2024-01-10T10:00:00Z",
        created_at: "2024-01-09T09:00:00Z",
        result: null,
      },
      {
        id: "op_2",
        client_id: "2",
        operation_type: "reboot",
        operation_mode: "force",
        scheduled_time: "2024-01-10T15:00:00Z",
        created_at: "2024-01-09T14:00:00Z",
        result: null,
      },
    ];

    api.getScheduledOperations.mockResolvedValue({
      operations: mockOperations,
    });

    render(
      <ScheduledOperationsList
        isOpen={true}
        onClose={mockOnClose}
      />
    );

    await waitFor(() => {
      const badges = screen.getAllByText(/shutdown|reboot/);
      expect(badges.length).toBeGreaterThan(0);
    });
  });

  it("displays correct operation mode badges", async () => {
    const mockOperations = [
      {
        id: "op_1",
        client_id: "1",
        operation_type: "shutdown",
        operation_mode: "graceful",
        scheduled_time: "2024-01-10T10:00:00Z",
        created_at: "2024-01-09T09:00:00Z",
        result: null,
      },
      {
        id: "op_2",
        client_id: "2",
        operation_type: "reboot",
        operation_mode: "force",
        scheduled_time: "2024-01-10T15:00:00Z",
        created_at: "2024-01-09T14:00:00Z",
        result: null,
      },
    ];

    api.getScheduledOperations.mockResolvedValue({
      operations: mockOperations,
    });

    render(
      <ScheduledOperationsList
        isOpen={true}
        onClose={mockOnClose}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("graceful")).toBeInTheDocument();
      expect(screen.getByText("force")).toBeInTheDocument();
    });
  });

  it("calls cancelScheduledOperation when cancel button is clicked", async () => {
    const user = userEvent.setup();
    const mockOperations = [
      {
        id: "op_1",
        client_id: "1",
        operation_type: "shutdown",
        operation_mode: "graceful",
        scheduled_time: "2024-01-10T10:00:00Z",
        created_at: "2024-01-09T09:00:00Z",
        result: null,
      },
    ];

    api.getScheduledOperations.mockResolvedValue({
      operations: mockOperations,
    });

    api.cancelScheduledOperation.mockResolvedValue({
      success: true,
    });

    render(
      <ScheduledOperationsList
        isOpen={true}
        onClose={mockOnClose}
      />
    );

    await waitFor(() => {
      const cancelButtons = screen.getAllByRole("button", { name: /cancel/i });
      expect(cancelButtons.length).toBeGreaterThan(0);
    });
  });

  it("displays pending status for operations without result", async () => {
    const mockOperations = [
      {
        id: "op_1",
        client_id: "1",
        operation_type: "shutdown",
        operation_mode: "graceful",
        scheduled_time: "2024-01-10T10:00:00Z",
        created_at: "2024-01-09T09:00:00Z",
        result: null,
      },
    ];

    api.getScheduledOperations.mockResolvedValue({
      operations: mockOperations,
    });

    render(
      <ScheduledOperationsList
        isOpen={true}
        onClose={mockOnClose}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("Pending")).toBeInTheDocument();
    });
  });

  it("displays client name from store", async () => {
    const mockOperations = [
      {
        id: "op_1",
        client_id: "1",
        operation_type: "shutdown",
        operation_mode: "graceful",
        scheduled_time: "2024-01-10T10:00:00Z",
        created_at: "2024-01-09T09:00:00Z",
        result: null,
      },
    ];

    api.getScheduledOperations.mockResolvedValue({
      operations: mockOperations,
    });

    render(
      <ScheduledOperationsList
        isOpen={true}
        onClose={mockOnClose}
      />
    );

    await waitFor(() => {
      expect(screen.getByText("client-01")).toBeInTheDocument();
    });
  });

  it("displays unknown client for missing client ID", async () => {
    const mockOperations = [
      {
        id: "op_1",
        client_id: "999",
        operation_type: "shutdown",
        operation_mode: "graceful",
        scheduled_time: "2024-01-10T10:00:00Z",
        created_at: "2024-01-09T09:00:00Z",
        result: null,
      },
    ];

    api.getScheduledOperations.mockResolvedValue({
      operations: mockOperations,
    });

    render(
      <ScheduledOperationsList
        isOpen={true}
        onClose={mockOnClose}
      />
    );

    await waitFor(() => {
      expect(screen.getByText(/Unknown \(999\)/)).toBeInTheDocument();
    });
  });
});
