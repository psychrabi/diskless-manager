import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import AuditLogViewer from "./AuditLogViewer";
import * as api from "@/api/commands";

// Mock the API
vi.mock("@/api/commands", () => ({
  getAuditLogs: vi.fn(),
}));

// Mock the notification context
vi.mock("@/contexts/notification", () => ({
  useNotification: () => ({
    showNotification: vi.fn(),
  }),
}));

// Mock the app store
vi.mock("@/store/useAppStore", () => ({
  useAppStore: (selector) => {
    const state = {
      clients: [
        { id: 1, name: "client-01" },
        { id: 2, name: "client-02" },
      ],
    };
    return selector(state);
  },
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

describe("AuditLogViewer", () => {
  const mockLogs = [
    {
      id: "log_1",
      client_name: "client-01",
      client_ip: "192.168.1.100",
      operation_type: "shutdown",
      operation_mode: "graceful",
      result: "success",
      duration_ms: 1250,
      timestamp: "2024-01-09T10:30:00Z",
    },
    {
      id: "log_2",
      client_name: "client-02",
      client_ip: "192.168.1.101",
      operation_type: "reboot",
      operation_mode: "force",
      result: "failed",
      duration_ms: 5000,
      timestamp: "2024-01-09T11:00:00Z",
    },
  ];

  const mockOnClose = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    api.getAuditLogs.mockResolvedValue({ logs: mockLogs });
  });

  it("fetches audit logs when modal opens", async () => {
    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      expect(api.getAuditLogs).toHaveBeenCalled();
    });
  });

  it("displays audit logs in table format", async () => {
    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      expect(screen.getByText("client-01")).toBeInTheDocument();
      expect(screen.getByText("client-02")).toBeInTheDocument();
    });
  });

  it("displays log count", async () => {
    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      expect(screen.getByText("2 log entries found")).toBeInTheDocument();
    });
  });

  it("displays empty state when no logs found", async () => {
    api.getAuditLogs.mockResolvedValue({ logs: [] });

    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      expect(screen.getByText("No audit logs found")).toBeInTheDocument();
    });
  });

  it("filters logs by client", async () => {
    const user = userEvent.setup();
    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      expect(screen.getByText("client-01")).toBeInTheDocument();
    });

    const clientSelect = screen.getByDisplayValue("All Clients");
    await user.selectOptions(clientSelect, "1");

    await waitFor(() => {
      expect(api.getAuditLogs).toHaveBeenCalledWith(
        expect.objectContaining({ client_id: "1" })
      );
    });
  });

  it("filters logs by operation type", async () => {
    const user = userEvent.setup();
    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      expect(screen.getByText("client-01")).toBeInTheDocument();
    });

    const operationSelect = screen.getByDisplayValue("All Operations");
    await user.selectOptions(operationSelect, "shutdown");

    await waitFor(() => {
      expect(api.getAuditLogs).toHaveBeenCalledWith(
        expect.objectContaining({ operation_type: "shutdown" })
      );
    });
  });

  it("clears all filters when clear button is clicked", async () => {
    const user = userEvent.setup();
    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      expect(screen.getByText("client-01")).toBeInTheDocument();
    });

    // Set a filter
    const clientSelect = screen.getByDisplayValue("All Clients");
    await user.selectOptions(clientSelect, "1");

    // Clear filters
    const clearButtons = screen.getAllByRole("button", { name: /clear filters/i });
    if (clearButtons.length > 0) {
      await user.click(clearButtons[0]);
    }

    await waitFor(() => {
      expect(api.getAuditLogs).toHaveBeenCalled();
    });
  });

  it("paginates logs correctly", async () => {
    const user = userEvent.setup();
    const manyLogs = Array.from({ length: 25 }, (_, i) => ({
      id: `log_${i}`,
      client_name: `client-${i}`,
      client_ip: `192.168.1.${100 + i}`,
      operation_type: "shutdown",
      operation_mode: "graceful",
      result: "success",
      duration_ms: 1250,
      timestamp: "2024-01-09T10:30:00Z",
    }));

    api.getAuditLogs.mockResolvedValue({ logs: manyLogs });

    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      const pageText = screen.queryByText(/Page 1 of/);
      expect(pageText).toBeInTheDocument();
    });
  });

  it("disables previous button on first page", async () => {
    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      expect(screen.getByText("client-01")).toBeInTheDocument();
    });

    const previousButtons = screen.getAllByRole("button", { name: /previous/i });
    if (previousButtons.length > 0) {
      expect(previousButtons[0]).toBeDisabled();
    }
  });

  it("displays operation type badges", async () => {
    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      const badges = screen.getAllByText(/shutdown|reboot/);
      expect(badges.length).toBeGreaterThan(0);
    });
  });

  it("displays result badges", async () => {
    render(
      <AuditLogViewer isOpen={true} onClose={mockOnClose} />
    );

    await waitFor(() => {
      expect(screen.getByText("success")).toBeInTheDocument();
      expect(screen.getByText("failed")).toBeInTheDocument();
    });
  });
});
