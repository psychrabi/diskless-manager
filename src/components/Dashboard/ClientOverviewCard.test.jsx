import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ClientOverviewCard from "./ClientOverviewCard";

const state = vi.hoisted(() => ({
  metrics: {
    clients: [
      { ip: "192.168.1.101", status: "Online" },
      { ip: "192.168.1.102", status: "Offline" },
      { ip: "192.168.1.103", status: "Online" },
    ],
  },
}));

vi.mock("@/api/modules/dashboard", () => ({
  getClientOverview: vi.fn().mockResolvedValue({ total: 3, online: 3, offline: 0 }),
}));
vi.mock("@/contexts/useMetrics", () => ({
  useMetrics: () => ({ metrics: state.metrics }),
}));
vi.mock("@/store/useToastStore", () => ({
  useToastStore: () => ({ error: vi.fn() }),
}));

describe("ClientOverviewCard", () => {
  beforeEach(() => {
    state.metrics = {
      clients: [
        { ip: "192.168.1.101", status: "Online" },
        { ip: "192.168.1.102", status: "Offline" },
        { ip: "192.168.1.103", status: "Online" },
      ],
    };
  });

  it("counts online clients from live iSCSI session metrics", async () => {
    render(<ClientOverviewCard />);

    const onlineLabel = await screen.findByText("Online Clients:");
    const offlineLabel = screen.getByText("Offline Clients:");

    expect(onlineLabel.nextElementSibling).toHaveTextContent("2");
    expect(offlineLabel.nextElementSibling).toHaveTextContent("1");
  });
});
