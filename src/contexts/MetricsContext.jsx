import React, { useEffect, useRef, useState } from "react";
import { MetricsContext } from "./metricsContext";

export const MetricsProvider = ({ children }) => {
  const [metrics, setMetrics] = useState(null);
  const [error, setError] = useState("");
  const [isConnected, setIsConnected] = useState(false);
  const wsRef = useRef(null);
  const reconnectTimeoutRef = useRef(null);
  const reconnectAttemptsRef = useRef(0);
  const maxReconnectAttempts = 10;

  useEffect(() => {
    let unmounted = false;

    const closeWebSocket = () => {
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
        reconnectTimeoutRef.current = null;
      }

      if (
        wsRef.current &&
        (wsRef.current.readyState === WebSocket.OPEN ||
          wsRef.current.readyState === WebSocket.CONNECTING)
      ) {
        wsRef.current.onclose = null;
        wsRef.current.close();
      }

      wsRef.current = null;
      setIsConnected(false);
    };

    const connectWebSocket = () => {
      if (unmounted) {
        return;
      }

      // Get auth token
      const token = localStorage.getItem("authToken");
      if (!token) {
        setError("Not authenticated");
        return;
      }

      // Don't reconnect if we already have an open connection
      if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
        console.log("WebSocket already open, skipping connection");
        return;
      }

      // Connect to WebSocket
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = `${protocol}//127.0.0.1:8080/ws/metrics?token=${encodeURIComponent(token)}`;

      console.log(
        `Establishing global WebSocket connection (attempt ${reconnectAttemptsRef.current + 1}/${maxReconnectAttempts})`
      );

      try {
        const ws = new WebSocket(wsUrl);

        ws.onopen = () => {
          if (unmounted) {
            return;
          }
          console.log("Global WebSocket connected successfully");
          setIsConnected(true);
          setError("");
          reconnectAttemptsRef.current = 0; // Reset on successful connection
        };

        ws.onmessage = (event) => {
          try {
            const data = JSON.parse(event.data);
            setMetrics(data);
          } catch (err) {
            console.error("Failed to parse WebSocket message:", err);
          }
        };

        ws.onerror = (err) => {
          if (unmounted) {
            return;
          }
          console.error("Global WebSocket error:", err);
          setError("WebSocket connection error");
          setIsConnected(false);
        };

        ws.onclose = () => {
          if (unmounted) {
            return;
          }

          console.log("Global WebSocket disconnected");
          setIsConnected(false);

          // Attempt to reconnect with exponential backoff
          if (reconnectAttemptsRef.current < maxReconnectAttempts) {
            const backoffDelay = Math.min(1000 * Math.pow(2, reconnectAttemptsRef.current), 30000);
            console.log(
              `Scheduling reconnection in ${backoffDelay}ms (attempt ${reconnectAttemptsRef.current + 1}/${maxReconnectAttempts})`
            );
            reconnectAttemptsRef.current += 1;
            reconnectTimeoutRef.current = setTimeout(() => {
              connectWebSocket();
            }, backoffDelay);
          } else {
            console.error("Max reconnection attempts reached");
            setError("Failed to connect to metrics stream after multiple attempts");
          }
        };

        wsRef.current = ws;
      } catch (err) {
        console.error("Failed to create global WebSocket:", err);
        setError("Failed to connect to metrics stream");
      }
    };

    const handleAuthLogout = () => {
      reconnectAttemptsRef.current = 0;
      closeWebSocket();
      setMetrics(null);
      setError("Not authenticated");
    };

    const handleAuthLogin = () => {
      reconnectAttemptsRef.current = 0;
      closeWebSocket();
      setError("");
      connectWebSocket();
    };

    window.addEventListener("auth:logout", handleAuthLogout);
    window.addEventListener("auth:login", handleAuthLogin);
    connectWebSocket();

    return () => {
      unmounted = true;
      window.removeEventListener("auth:logout", handleAuthLogout);
      window.removeEventListener("auth:login", handleAuthLogin);
      closeWebSocket();
    };
  }, []);

  return (
    <MetricsContext.Provider value={{ metrics, error, isConnected }}>
      {children}
    </MetricsContext.Provider>
  );
};
