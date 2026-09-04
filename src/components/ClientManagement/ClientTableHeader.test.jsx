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

    expect(screen.getByText(/Disk Read/)).toBeInTheDocument();
    expect(screen.getByText(/Disk Write/)).toBeInTheDocument();
    expect(screen.queryByText(/Network/)).not.toBeInTheDocument();
    expect(screen.queryByText(/iSCSI/)).not.toBeInTheDocument();
  });
});
