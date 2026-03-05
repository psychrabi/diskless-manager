import { useEffect, useState, useRef } from "react";

export const useMetricsWebSocket = () => {
  const [metrics, setMetrics] = useState(null);
  const [error, setError] = useState(() =>
    localStorage.getItem("authToken") ? "" : "Not authenticated"
  );
  const [isConnected, setIsConnected] = useState(false);
  const wsRef = useRef(null);
  const reconnectTimeoutRef = useRef(null);

  useEffect(() => {
    // Get auth token
    const token = localStorage.getItem("authToken");
    if (!token) {
      return;
    }

    const connectWebSocket = () => {
      // Connect to WebSocket
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = `${protocol}//127.0.0.1:8080/ws/metrics?token=${encodeURIComponent(token)}`;

      console.log("Attempting to connect to WebSocket:", wsUrl);

      try {
        const ws = new WebSocket(wsUrl);

        ws.onopen = () => {
          console.log("WebSocket connected successfully");
          setIsConnected(true);
          setError("");
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
          console.error("WebSocket error:", err);
          setError("WebSocket connection error");
          setIsConnected(false);
        };

        ws.onclose = () => {
          console.log("WebSocket disconnected");
          setIsConnected(false);
          // Attempt to reconnect after 3 seconds
          reconnectTimeoutRef.current = setTimeout(() => {
            console.log("Attempting to reconnect WebSocket...");
            connectWebSocket();
          }, 3000);
        };

        wsRef.current = ws;
      } catch (err) {
        console.error("Failed to create WebSocket:", err);
        setError("Failed to connect to metrics stream");
      }
    };

    connectWebSocket();

    return () => {
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
      if (wsRef.current) {
        wsRef.current.close();
      }
    };
  }, []);

  return { metrics, error, isConnected };
};
