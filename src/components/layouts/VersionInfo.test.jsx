import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import VersionInfo from "./VersionInfo";
import { useAppStore } from "@/store/useAppStore";

describe("VersionInfo", () => {
  afterEach(() => {
    useAppStore.setState({ serverInfo: null });
  });

  it("shows the server version and build date", () => {
    useAppStore.setState({
      serverInfo: { app_version: "1.0.0", build_date: "2026-11-15T14:55:01Z" },
    });
    render(<VersionInfo />);
    expect(screen.getByTestId("version-info").textContent).toBe(
      "v1.0.0 · built 2026-11-15T14:55:01Z",
    );
  });

  it("omits the build date when the server does not provide one", () => {
    useAppStore.setState({ serverInfo: { app_version: "1.0.0" } });
    render(<VersionInfo />);
    expect(screen.getByTestId("version-info").textContent).toBe("v1.0.0");
  });

  it("renders nothing until server info arrives", () => {
    useAppStore.setState({ serverInfo: null });
    const { container } = render(<VersionInfo />);
    expect(container).toBeEmptyDOMElement();
  });
});
