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

    expect(screen.getByRole("columnheader", { name: "Read Speed (MB/s)" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Write Speed (MB/s)" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Total Read" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Total Write" })).toBeInTheDocument();
    expect(screen.queryByText("Restore Point")).not.toBeInTheDocument();
    expect(screen.queryByText("Boot disk")).not.toBeInTheDocument();
    expect(screen.queryByText(/Network/)).not.toBeInTheDocument();
    expect(screen.queryByText(/iSCSI/)).not.toBeInTheDocument();
  });
});
