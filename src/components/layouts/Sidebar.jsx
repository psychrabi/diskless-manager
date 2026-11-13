import { Link, NavLink, useLocation } from "react-router-dom";
import { Boxes } from "lucide-react";
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
        (item) => user?.role === "admin" || !item.adminOnly,
      ),
    }))
    .filter((section) => section.items.length > 0);
  const closeMobile = () => setOpenMobile(false);

  return (
    <SidebarRoot collapsible="icon" variant="inset">
      <SidebarHeader className="px-3 pt-3 pb-1 max-md:pr-12">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              size="lg"
              render={<Link to="/" />}
              onClick={closeMobile}
              aria-label="Diskless Manager home"
              className="h-14 rounded-xl px-2 hover:bg-sidebar-accent/60"
            >
              <div className="relative flex size-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-primary to-primary/70 shadow-sm ring-1 ring-primary/20">
                <Boxes className="size-[18px] text-primary-foreground" />
                <span className="absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full border-2 border-sidebar bg-success" />
              </div>
              <div className="grid min-w-0 flex-1 text-left leading-tight group-data-[collapsible=icon]:hidden">
                <span className="truncate text-[15px] font-semibold tracking-[-0.015em]">Diskless Manager</span>
                <span className="mt-1 truncate text-[11px] font-normal text-muted-foreground">Boot server control</span>
              </div>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarSeparator className="mx-4 opacity-70" />
      <SidebarContent className="group-data-[collapsible=icon]:overflow-y-auto">
        <nav aria-label="Main navigation" className="py-1">
          {visibleSections.map((section) => (
            <SidebarGroup key={section.label} className="px-3 py-1.5">
              <SidebarGroupLabel className="h-7 px-2.5 text-[10px] font-semibold tracking-[0.14em] uppercase text-muted-foreground/80">
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
                      className="relative h-9 rounded-lg px-2.5 text-[13px] text-sidebar-foreground/75 before:absolute before:inset-y-2 before:left-0 before:w-[3px] before:rounded-full before:bg-primary before:opacity-0 data-active:bg-sidebar-accent data-active:font-medium data-active:text-sidebar-accent-foreground data-active:before:opacity-100 [&_svg]:text-sidebar-foreground/45 data-active:[&_svg]:text-primary"
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
      <SidebarSeparator className="mx-4 opacity-70" />
      <SidebarFooter className="p-3 pt-2">
        <NavUser />
      </SidebarFooter>
      <SidebarRail />
    </SidebarRoot>
  );
}
