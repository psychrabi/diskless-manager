import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { AuthContext } from "@/contexts/auth";
import ProtectedRoute from "./ProtectedRoute";

function renderPath(role, initialPath = "/installer-media") {
  return render(
    <AuthContext.Provider value={{ loading: false, user: { role }, token: "token" }}>
      <MemoryRouter initialEntries={[initialPath]}>
        <Routes>
          <Route path="/" element={<p>Home</p>} />
          <Route path="/installer-media" element={<ProtectedRoute><p>Installer Media page</p></ProtectedRoute>} />
        </Routes>
      </MemoryRouter>
    </AuthContext.Provider>,
  );
}

describe("installer media route access", () => {
  it("redirects non-admins away from the page", async () => {
    renderPath("user");
    expect(await screen.findByText("Home")).toBeInTheDocument();
    expect(screen.queryByText("Installer Media page")).not.toBeInTheDocument();
  });

  it("redirects non-admins when the protected page has a trailing slash", async () => {
    renderPath("user", "/installer-media/");
    expect(await screen.findByText("Home")).toBeInTheDocument();
    expect(screen.queryByText("Installer Media page")).not.toBeInTheDocument();
  });

  it("allows admins to reach the page", () => {
    renderPath("admin");
    expect(screen.getByText("Installer Media page")).toBeInTheDocument();
  });
});
