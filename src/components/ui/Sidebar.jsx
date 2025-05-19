import React from "react";
import { NavLink } from "react-router-dom";
import { Power, LayoutDashboard, Monitor, HardDrive } from "lucide-react";

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

const Sidebar = () => (
  <aside className="h-screen w-56 bg-gray-900 text-gray-100 flex flex-col justify-between fixed left-0 top-0 z-40">
    <div>
      <div className="p-4 font-bold tracking-tight">Diskless Boot Manager</div>
      <nav className="flex flex-col gap-1">
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            className={({ isActive }) =>
              `flex items-center px-4 py-2 rounded transition-colors ${
                isActive
                  ? "bg-gray-800 text-blue-400"
                  : "hover:bg-gray-800 text-gray-100"
              }`
            }
          >
            {item.icon}
            {item.label}
          </NavLink>
        ))}
      </nav>
    </div>
    <button className="flex items-center px-4 py-2 m-4 rounded bg-red-600 hover:bg-red-700 text-white transition-colors">
      <Power className="w-4 h-4 mr-2" />
      Exit
    </button>
  </aside>
);

export default Sidebar;
