import { Button } from "@/components/ui";
import { useAuth } from "@/contexts/auth";
import { useTheme } from "@/contexts/theme";
import { LogOut, Menu, Moon, Sun, User } from "lucide-react";
import { useNavigate } from "react-router-dom";
import Brand from "./Brand";

const Header = ({ onToggleSidebar }) => {
  const navigate = useNavigate();
  const { logout, user } = useAuth();
  const { theme, setTheme } = useTheme();

  const toggleTheme = () => setTheme(theme === "dark" ? "light" : "dark");

  return (
    <header className="bg-base-100/95 backdrop-blur-sm border-b border-base-200/50 px-6 py-4 shadow-sm sticky top-0 z-30">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          {/* Mobile menu button */}
          <Button
            variant="ghost"
            size="icon"
            className="lg:hidden"
            aria-label="Open sidebar"
            onClick={onToggleSidebar}
          >
            <Menu className="h-5 w-5" />
          </Button>

          {/* Brand and title */}
          <Brand
            subtitle="Boot Server Control Panel"
            titleClassName="text-heading-md"
            subtitleClassName="hidden sm:block text-body-sm"
          />
        </div>

        {/* Actions */}
        <div className="flex items-center gap-2">
          {/* Theme toggle */}
          <Button
            variant="ghost"
            size="icon"
            onClick={toggleTheme}
            title={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}
            className="hover:bg-base-200"
          >
            {theme === "dark" ? (
              <Sun className="h-4 w-4" />
            ) : (
              <Moon className="h-4 w-4" />
            )}
          </Button>

          {/* User info */}
          <div className="hidden sm:flex items-center gap-3 px-3 py-2 bg-base-200/50 rounded-lg">
            <div className="w-8 h-8 bg-primary/10 rounded-full flex items-center justify-center">
              <User className="h-4 w-4 text-primary" />
            </div>
            <div className="text-right">
              <p className="text-body-sm font-medium text-base-content capitalize">
                {user?.role || "Administrator"}
              </p>
              <p className="text-caption text-base-content/60">
                System Admin
              </p>
            </div>
          </div>

          {/* Logout button */}
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              logout();
              navigate("/login");
            }}
            className="gap-2"
            title="Sign out"
          >
            <LogOut className="h-4 w-4" />
            <span className="hidden sm:inline">Sign Out</span>
          </Button>
        </div>
      </div>
    </header>
  );
};

export default Header;
