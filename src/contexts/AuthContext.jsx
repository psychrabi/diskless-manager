import { useToastStore } from "@/store/useToastStore";
import { useCallback, useEffect, useState } from "react";
import { setAuthToken } from "@/api/client";
import { validateAuthToken } from "@/api/commands";
import { AuthContext } from "./auth";

const AUTH_TOKEN_KEY = "authToken";
const AUTH_USER_KEY = "user";

export const AuthProvider = ({ children }) => {
  const [user, setUser] = useState(null);
  const [token, setToken] = useState(null);
  const [loading, setLoading] = useState(true);
  const { error, success } = useToastStore();

  // Define logout and validateToken BEFORE useEffect to avoid TDZ / uninitialized variable errors
  const logout = useCallback(() => {
    setUser(null);
    setToken(null);
    setAuthToken(null);
    localStorage.removeItem(AUTH_USER_KEY);
    window.dispatchEvent(new Event("auth:logout"));
    success("Authentication", "Logout Successful");
  }, [success]);

  const validateToken = useCallback(
    async () => {
      try {
        // Validate the token through the API
        await validateAuthToken();
        // Token is valid, do nothing
      } catch (err) {
        error(err.message || "Your session has expired. Please log in again.");
        logout();
      }
    },
    [logout, error]
  );

  useEffect(() => {
    (async () => {
      const storedToken = localStorage.getItem(AUTH_TOKEN_KEY);
      const storedUser = localStorage.getItem(AUTH_USER_KEY);

      if (storedToken && storedUser) {
        try {
          const parsedUser = JSON.parse(storedUser);
          setAuthToken(storedToken);
          setToken(storedToken);
          setUser(parsedUser);

          await validateToken();
        } catch {
          setAuthToken(null);
          localStorage.removeItem(AUTH_USER_KEY);
        }
      }
      setLoading(false);
    })();
  }, [validateToken]);

  const login = (userData, authToken) => {
    setUser(userData);
    setToken(authToken);
    setAuthToken(authToken);
    localStorage.setItem(AUTH_USER_KEY, JSON.stringify(userData));
    window.dispatchEvent(new Event("auth:login"));
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
