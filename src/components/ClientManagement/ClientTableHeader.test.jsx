import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ClientTableHeader from "./ClientTableHeader";

describe("ClientTableHeader", () => {
  it("shows one disk throughput read and write pair", () => {
    render(
      <table>
        <thead>
          <ClientTableHeader />
        </thead>
      </table>
    );

    expect(screen.getByRole("columnheader", { name: "Throughput (MB/s)" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Total I/O" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Client" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Mode" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Actions" })).toBeInTheDocument();
    expect(screen.queryByText("Restore Point")).not.toBeInTheDocument();
    expect(screen.queryByText("Boot disk")).not.toBeInTheDocument();
    expect(screen.queryByText(/Network/)).not.toBeInTheDocument();
    expect(screen.queryByText(/iSCSI/)).not.toBeInTheDocument();
  });
});
