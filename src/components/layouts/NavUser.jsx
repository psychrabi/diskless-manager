import { ChevronsUpDown, KeyRound, LogOut, Power, Settings } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { exit } from "@tauri-apps/plugin-process";
import { useAuth } from "@/contexts/auth";
import { useConfirm } from "@/contexts/confirmDialog";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem,
  DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { SidebarMenu, SidebarMenuButton, SidebarMenuItem, useSidebar } from "@/components/ui/sidebar";

// The account footer from sidebar-07, connected to this application's session.
export default function NavUser() {
  const { user, logout } = useAuth();
  const { isMobile, setOpenMobile } = useSidebar();
  const navigate = useNavigate();
  const confirm = useConfirm();
  const name = user?.username || "User";
  const role = user?.role || "Signed in";
  const goTo = (path) => { setOpenMobile(false); navigate(path); };

  const handleExit = async () => {
    const confirmed = await confirm({
      title: "Exit Application",
      description: "Are you sure you want to exit the Diskless Manager?",
      confirmText: "Exit", confirmVariant: "destructive", cancelText: "Cancel",
    });
    if (confirmed) {
      try { await exit(0); }
      catch (error) { console.error("Failed to exit application:", error); }
    }
  };

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger render={<SidebarMenuButton size="lg" className="aria-expanded:bg-sidebar-accent" />} aria-label={`Account menu for ${name}`}>
            <Avatar className="size-8 rounded-lg">
              <AvatarFallback className="rounded-lg bg-sidebar-accent text-sidebar-accent-foreground">{name.slice(0, 2).toUpperCase()}</AvatarFallback>
            </Avatar>
            <div className="grid min-w-0 flex-1 text-left text-sm leading-tight group-data-[collapsible=icon]:hidden">
              <span className="truncate font-medium">{name}</span>
              <span className="mt-1 truncate text-xs text-muted-foreground capitalize">{role}</span>
            </div>
            <ChevronsUpDown className="ml-auto size-4 group-data-[collapsible=icon]:hidden" />
          </DropdownMenuTrigger>
          <DropdownMenuContent className="min-w-56" side={isMobile ? "top" : "right"} align="end" sideOffset={8}>
            <DropdownMenuGroup>
              <DropdownMenuLabel className="px-2 py-2">
                <span className="block text-sm font-medium text-foreground">{name}</span>
                <span className="text-xs capitalize">{role}</span>
              </DropdownMenuLabel>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => goTo("/application-settings")}><Settings />Application settings</DropdownMenuItem>
            <DropdownMenuItem onClick={() => goTo("/license")}><KeyRound />License</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => { logout(); goTo("/login"); }}><LogOut />Sign out</DropdownMenuItem>
            <DropdownMenuItem variant="destructive" onClick={handleExit}><Power />Exit application</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
