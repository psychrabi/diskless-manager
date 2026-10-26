import { render, screen, within } from "@testing-library/react";
import { it, expect, vi } from "vitest";
import { fireEvent } from "@testing-library/react";
import ClientTableHeader from "./ClientTableHeader";
import ClientTableRow from "./ClientTableRow";

it("aligns client metrics with headers and keeps internal paths out of table cells", () => {
  const client = { id: "c1", name: "PC001", mac: "00:11:22:33:44:55", ip: "192.168.1.101", status: "Offline", master: "Windows", snapshot: "pool/windows@ready", block_store: "/dev/zvol/pool/pc001", keep_writeback: false };
  render(<table><thead><ClientTableHeader /></thead><tbody><tr><ClientTableRow client={client} clientMetrics={{ iscsi: { read_speed_mbps: 12, write_speed_mbps: 3, total_read_bytes: 1610612736, total_write_bytes: 1048576 }, uptime_seconds: 0 }} /></tr></tbody></table>);
  const cells = screen.getAllByRole("cell").filter((cell) => cell.classList.contains("xl:table-cell"));
  const headers = screen.getAllByRole("columnheader");
  expect(cells).toHaveLength(headers.length);
  expect(headers.map((h) => h.textContent)).toEqual([
    "Name",
    "MAC Address",
    "IP Address",
    "I/O (MB/s)",
    "I/O (Total)",
    "Image",
    "Mode",
    "Uptime",
    "Actions",
  ]);
  const throughput = within(cells[3]);
  expect(throughput.getByText("12.00")).toBeInTheDocument();
  expect(throughput.getByText("3.00")).toBeInTheDocument();
  const totals = within(cells[4]);
  expect(totals.getByText("1.5 GB")).toBeInTheDocument();
  expect(totals.getByText("1.0 MB")).toBeInTheDocument();
  expect(screen.getAllByText("PC001")).toHaveLength(2);
  expect(screen.getAllByText("00:11:22:33:44:55")).toHaveLength(2);
  expect(screen.getAllByText("192.168.1.101")).toHaveLength(2);
  expect(screen.queryByText(client.snapshot)).not.toBeInTheDocument();
  expect(screen.queryByText(client.block_store)).not.toBeInTheDocument();
});

it("keeps write and uptime icons visible and centers the uptime content", () => {
  const client = { id: "c1", name: "PC001", mac: "00:11:22:33:44:55", ip: "192.168.1.101", status: "Online", master: "Windows", snapshot: "pool/windows@ready" };
  render(<table><tbody><tr><ClientTableRow client={client} clientMetrics={{ iscsi: { write_speed_mbps: 3 }, uptime_seconds: 60 }} /></tr></tbody></table>);
  const cells = screen.getAllByRole("cell").filter((cell) => cell.classList.contains("xl:table-cell"));
  const writeIcon = cells[3].querySelector('[title="Write speed (MB/s)"] svg');
  const uptimeIcon = cells[7].querySelector("svg");
  expect(writeIcon.className.baseVal).toContain("text-muted-foreground");
  expect(uptimeIcon.className.baseVal).toContain("text-muted-foreground");
  expect(cells[7].firstElementChild.className).toContain("justify-center");
});

it("distinguishes legacy Super and recovery-required clients from managed Super", () => {
  const client = { id: "c1", name: "PC001", mac: "00:11:22:33:44:55", ip: "192.168.1.101", status: "Offline", master: "Windows", mode: "super", legacy_super_state: "recovery_required" };
  render(<table><tbody><tr><ClientTableRow client={client} /></tr></tbody></table>);
  expect(screen.getAllByText("Legacy Super recovery required")).toHaveLength(2);
  expect(screen.queryByText("Super")).not.toBeInTheDocument();
});

it("renders key client details and actions in the narrow-screen card", () => {
  const client = { id: "c1", name: "PC001", mac: "00:11:22:33:44:55", ip: "192.168.1.101", status: "Offline", master: "Ubuntu", snapshot: "pool/ubuntu@ready", keep_writeback: false };
  const onMoreActions = vi.fn();
  render(<table><tbody><tr><ClientTableRow client={client} clientMetrics={{ iscsi: { read_speed_mbps: 12, write_speed_mbps: 3 }, uptime_seconds: 60 }} onMoreActions={onMoreActions} /></tr></tbody></table>);
  const mobileCard = document.querySelector("[data-testid='client-mobile-card']");
  expect(mobileCard).toBeInTheDocument();
  expect(within(mobileCard).getByText("PC001")).toBeInTheDocument();
  expect(within(mobileCard).getByText("00:11:22:33:44:55")).toBeInTheDocument();
  expect(within(mobileCard).getByText("192.168.1.101")).toBeInTheDocument();
  expect(within(mobileCard).getByTitle("Reboot client")).toBeInTheDocument();
  fireEvent.click(within(mobileCard).getByRole("button", { name: "More actions" }));
  expect(onMoreActions).toHaveBeenCalledWith(expect.anything(), client);
});
