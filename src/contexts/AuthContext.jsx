import { useToastStore } from "@/store/useToastStore";
import { useCallback, useEffect, useState } from "react";
import { AuthContext } from "./auth";

export const AuthProvider = ({ children }) => {
  const [user, setUser] = useState(null);
  const [token, setToken] = useState(null);
  const [loading, setLoading] = useState(true);
  const { error } = useToastStore();

  // Define logout and validateToken BEFORE useEffect to avoid TDZ / uninitialized variable errors
  const logout = useCallback(() => {
    setUser(null);
    setToken(null);
    localStorage.removeItem("authToken");
    localStorage.removeItem("user");
  }, []);

  const validateToken = useCallback(
    async (authToken) => {
      try {
        // For now, we'll keep using invoke for token validation since it's not available through the API
        // In a future iteration, we could implement token validation through the API
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("validate_auth_token", { token: authToken });
        // Token is valid, do nothing
      } catch (err) {
        error(
          err.message || "Your session has expired. Please log in again.",
        );
        logout();
      }
    },
    [logout, error],
  );

  useEffect(() => {
    (async () => {
      const storedToken = localStorage.getItem("authToken");
      const storedUser = localStorage.getItem("user");

      if (storedToken && storedUser) {
        try {
          const parsedUser = JSON.parse(storedUser);
          setToken(storedToken);
          setUser(parsedUser);

          await validateToken(storedToken);
        } catch {
          localStorage.removeItem("authToken");
          localStorage.removeItem("user");
        }
      }
      setLoading(false);
    })();
  }, [validateToken]);

  const login = (userData, authToken) => {
    setUser(userData);
    setToken(authToken);
    localStorage.setItem("authToken", authToken);
    localStorage.setItem("user", JSON.stringify(userData));
  };

  const value = {
    user,
    token,
    login,
    logout,
    loading,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};
