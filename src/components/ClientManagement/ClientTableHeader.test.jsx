import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ClientTableHeader from "./ClientTableHeader";

describe("ClientTableHeader", () => {
  it("shows one disk throughput read and write pair", () => {
    const { container } = render(
      <table>
        <thead>
          <ClientTableHeader />
        </thead>
      </table>
    );

    expect(container.querySelector("thead").textContent).toContain("I/O (MB/s)");
    expect(container.querySelector("thead").textContent).toContain("I/O (Total)");
    expect(container.querySelector("thead").textContent).toContain("Name");
    expect(container.querySelector("thead").textContent).toContain("Mode");
    expect(container.querySelector("thead").textContent).toContain("Actions");
    expect(container.querySelector("thead").textContent).not.toMatch(/Restore Point|Boot disk|Network|iSCSI/);
  });
});
