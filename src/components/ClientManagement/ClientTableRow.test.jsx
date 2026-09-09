import { render, screen, within } from "@testing-library/react";
import { it, expect } from "vitest";
import ClientTableHeader from "./ClientTableHeader";
import ClientTableRow from "./ClientTableRow";

it("aligns disk totals with headers and keeps internal paths out of table cells", () => {
  const client = { id: "c1", name: "PC001", mac: "00:11:22:33:44:55", ip: "192.168.1.101", status: "Offline", master: "Windows", snapshot: "pool/windows@ready", block_store: "/dev/zvol/pool/pc001", keep_writeback: false };
  render(<table><thead><ClientTableHeader /></thead><tbody><tr><ClientTableRow client={client} clientMetrics={{ iscsi: { read_speed_mbps: 12, write_speed_mbps: 3, total_read_bytes: 1610612736, total_write_bytes: 1048576 }, uptime_seconds: 0 }} /></tr></tbody></table>);
  const cells = screen.getAllByRole("cell");
  const headers = screen.getAllByRole("columnheader");
  expect(cells).toHaveLength(headers.length);
  expect(headers.map((h) => h.textContent)).toEqual([
    "Client",
    "Throughput (MB/s)",
    "Total I/O",
    "Image",
    "Mode",
    "Uptime",
    "Actions",
  ]);
  const throughput = within(cells[1]);
  expect(throughput.getByText("12.00")).toBeInTheDocument();
  expect(throughput.getByText("3.00")).toBeInTheDocument();
  const totals = within(cells[2]);
  expect(totals.getByText("1.5 GB")).toBeInTheDocument();
  expect(totals.getByText("1.0 MB")).toBeInTheDocument();
  expect(screen.getByText("PC001")).toBeInTheDocument();
  expect(screen.getByText("00:11:22:33:44:55 • 192.168.1.101")).toBeInTheDocument();
  expect(screen.queryByText(client.snapshot)).not.toBeInTheDocument();
  expect(screen.queryByText(client.block_store)).not.toBeInTheDocument();
});
