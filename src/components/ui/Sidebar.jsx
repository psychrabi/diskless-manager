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
  <aside className="h-screen w-56 bg-base-200 text-base-content flex flex-col justify-between fixed left-0 top-0 z-40">
    <div>
      <div className="p-4 font-bold tracking-tight">Diskless Boot Manager</div>
      <ul className="menu menu-vertical px-2">
        {navItems.map((item) => (
          <li key={item.to}>
            <NavLink
              to={item.to}
              className={({ isActive }) =>
                `flex items-center ${isActive ? "active text-primary" : ""}`
              }
            >
              {item.icon}
              {item.label}
            </NavLink>
          </li>
        ))}
      </ul>
    </div>
    <button className="btn btn-error m-4 flex items-center">
      <Power className="w-4 h-4 mr-2" />
      Exit
    </button>
  </aside>
);

export default Sidebar;
