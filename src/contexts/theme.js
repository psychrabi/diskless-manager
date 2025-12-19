import { createContext, useContext } from "react";

export const ThemeContext = createContext({
  theme: "light",
  setTheme: () => {},
});

export const useTheme = () => useContext(ThemeContext);
