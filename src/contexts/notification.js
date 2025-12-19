import { createContext, useContext } from "react";

export const NotificationContext = createContext();

export const useNotification = () => useContext(NotificationContext);
