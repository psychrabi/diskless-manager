import { Link, NavLink, useLocation } from "react-router-dom";
import { Shield } from "lucide-react";
import {
  Sidebar as SidebarRoot, SidebarContent, SidebarFooter, SidebarGroup,
  SidebarGroupLabel, SidebarHeader, SidebarMenu, SidebarMenuButton,
  SidebarMenuItem, SidebarRail, SidebarSeparator, useSidebar,
} from "@/components/ui/sidebar";
import { navSections } from "./navigation";
import NavUser from "./NavUser";
import { useAuth } from "@/contexts/auth";

// Application navigation adapted from shadcn's sidebar-07 block.
export default function Sidebar() {
  const { pathname } = useLocation();
  const { setOpenMobile } = useSidebar();
  const { user } = useAuth();
  const visibleSections = navSections
    .map((section) => ({
      ...section,
      items: section.items.filter(
        (item) => user?.role === "admin" || !["users", "license"].includes(item.id),
      ),
    }))
    .filter((section) => section.items.length > 0);
  const closeMobile = () => setOpenMobile(false);

  return (
    <SidebarRoot collapsible="icon" variant="inset">
      <SidebarHeader className="max-md:pr-12">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" render={<Link to="/" />} onClick={closeMobile} aria-label="Diskless Manager home">
              <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                <Shield className="size-4" />
              </div>
              <div className="grid min-w-0 flex-1 text-left leading-tight group-data-[collapsible=icon]:hidden">
                <span className="truncate font-semibold">Diskless Manager</span>
                <span className="mt-1 truncate text-xs text-muted-foreground">Boot server control</span>
              </div>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarSeparator />
      <SidebarContent className="group-data-[collapsible=icon]:overflow-y-auto">
        <nav aria-label="Main navigation">
          {visibleSections.map((section) => (
            <SidebarGroup key={section.label}>
              <SidebarGroupLabel>{section.label}</SidebarGroupLabel>
              <SidebarMenu>
                {section.items.map((item) => (
                  <SidebarMenuItem key={item.id}>
                    <SidebarMenuButton
                      render={<NavLink to={item.to} end />}
                      isActive={pathname === item.to}
                      tooltip={item.label}
                      aria-label={item.label}
                      onClick={closeMobile}
                    >
                      <item.icon />
                      <span>{item.label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroup>
          ))}
        </nav>
      </SidebarContent>
      <SidebarSeparator />
      <SidebarFooter><NavUser /></SidebarFooter>
      <SidebarRail />
    </SidebarRoot>
  );
}
