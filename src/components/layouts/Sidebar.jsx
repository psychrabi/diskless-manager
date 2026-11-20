import { Link, NavLink, useLocation } from "react-router-dom";
import { Boxes } from "lucide-react";
import {
  Sidebar as SidebarRoot, SidebarContent, SidebarFooter, SidebarGroup,
  SidebarGroupLabel, SidebarHeader, SidebarMenu, SidebarMenuButton,
  SidebarMenuItem, SidebarRail, SidebarSeparator, useSidebar,
} from "@/components/ui/sidebar";
import { navSections } from "./navigation";
import NavUser from "./NavUser";
import VersionInfo from "./VersionInfo";
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
        (item) => user?.role === "admin" || !item.adminOnly,
      ),
    }))
    .filter((section) => section.items.length > 0);
  const closeMobile = () => setOpenMobile(false);

  return (
    <SidebarRoot collapsible="icon" variant="sidebar" className="border-r border-sidebar-border">
      <SidebarHeader className="px-3 pt-4 pb-2 max-md:pr-12">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              size="lg"
              render={<Link to="/" />}
              onClick={closeMobile}
              aria-label="Diskless Manager home"
              className="h-14 rounded-lg px-2 hover:bg-sidebar-accent/60"
            >
              <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                <Boxes className="size-[18px]" aria-hidden="true" />
              </div>
              <div className="grid min-w-0 flex-1 text-left leading-tight group-data-[collapsible=icon]:hidden">
                <span className="truncate text-sm font-semibold tracking-[-0.015em] text-sidebar-foreground">
                  Diskless Manager
                </span>
                <span className="mt-0.5 truncate text-xs text-sidebar-foreground/70">
                  Boot server
                </span>
              </div>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarSeparator className="mx-4 opacity-100" />
      <SidebarContent className="group-data-[collapsible=icon]:overflow-y-auto">
        <nav aria-label="Main navigation" className="py-2">
          {visibleSections.map((section) => (
            <SidebarGroup key={section.label} className="px-3 py-1">
              <SidebarGroupLabel className="h-7 px-2.5 text-xs font-medium text-sidebar-foreground/75">
                {section.label}
              </SidebarGroupLabel>
              <SidebarMenu className="gap-0.5">
                {section.items.map((item) => (
                  <SidebarMenuItem key={item.id}>
                    <SidebarMenuButton
                      render={<NavLink to={item.to} end />}
                      isActive={pathname === item.to}
                      tooltip={item.label}
                      aria-label={item.label}
                      onClick={closeMobile}
                      className="relative h-9 rounded-md px-2.5 text-[13px] text-sidebar-foreground/85 xl:h-10 xl:text-sm transition-colors duration-150 ease-out before:absolute before:inset-y-1.5 before:left-0 before:w-[2px] before:rounded-full before:bg-sidebar-primary before:opacity-0 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground data-active:bg-sidebar-accent data-active:font-medium data-active:text-sidebar-accent-foreground data-active:before:opacity-100 [&_svg]:text-sidebar-foreground/70 [&_svg]:transition-colors data-active:[&_svg]:text-sidebar-primary"
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
      <SidebarSeparator className="mx-4 opacity-100" />
      <SidebarFooter className="p-3 pt-2">
        <NavUser />
        <VersionInfo />
      </SidebarFooter>
      <SidebarRail />
    </SidebarRoot>
  );
}
