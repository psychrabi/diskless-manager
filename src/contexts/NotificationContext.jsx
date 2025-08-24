import { createContext, useContext, useState } from "react";

export const NotificationContext = createContext();

export const NotificationProvider = ({ children }) => {
  const [notification, setNotification] = useState({ message: "", type: "" });

  const showNotification = (message, type) => {
    const msgStr = typeof message === 'string'
      ? message
      : (message?.message || (typeof message?.toString === 'function' ? message.toString() : JSON.stringify(message)));
    setNotification({ message: msgStr, type });
    setTimeout(() => {
      setNotification({ message: "", type: "" }); // Clear after 10 seconds
    }, 5000);
  };

  const hideNotification = () => {
    setNotification({ message: "", type: "" });
  }

  return (
    <NotificationContext.Provider value={{ notification, showNotification, hideNotification }}>
      {children}
    </NotificationContext.Provider>
  );
};

export const useNotification = () => useContext(NotificationContext);