
import { HardDrive, LayoutDashboard, Monitor, Power } from "lucide-react";
import { NavLink } from "react-router-dom";
import { Button } from ".";
import { exit } from '@tauri-apps/plugin-process';

const navItems = [
  {
    to: "/",
    label: "Dashboard",
    icon: <LayoutDashboard className="w-4 h-4 mr-2" />,
  },
  {
    to: "/clients",
    label: "Clients",
    icon: <Monitor className="w-4 h-4 mr-2" />,
  },
  {
    to: "/masters",
    label: "Masters",
    icon: <HardDrive className="w-4 h-4 mr-2" />,
  },
];

const Sidebar = () => {
  const handleExit = async () => {
    try {
      await exit(0);
    } catch (error) {
      console.error('Failed to exit application:', error);
    }
  };

  return (
    <aside className="h-screen w-56 bg-base-200 text-base-content flex flex-col justify-between fixed left-0 top-0 z-40">
      <ul className="menu bg-base-200 rounded-box w-56">
        <li className="menu-title font-bold">Diskless Boot Manager</li>
        {navItems.map((item) => (
          <li key={item.to}>
            <NavLink
              to={item.to}
              className={({ isActive }) =>
                `${isActive ? "active text-primary" : ""}`
              }
            >
              {item.icon}
              {item.label}
            </NavLink>
          </li>
        ))}
      </ul>

      <Button variant="destructive" className="m-2" onClick={handleExit}>
        <Power className="w-4 h-4 mr-2" />
        Exit
      </Button>
    </aside>
  );
};

export default Sidebar;
