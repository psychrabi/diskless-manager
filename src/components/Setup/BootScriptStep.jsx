import { Code } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "../ui";
import { useServiceManager } from "@/hooks/useServiceManager";

const defaultScript = `#!ipxe

# Set server address
set server_ip \${next-server}

# Boot menu
:menu
menu Diskless Boot Menu
item --key w windows10  [w] Windows 10
item --key u ubuntu      [u] Ubuntu
item --key s shell       [s] iPXE Shell
item --key r reboot      [r] Reboot
choose --default windows10 --timeout 3000 target && goto \${target}

:windows10
sanboot iscsi:\${server_ip}::::iqn.2024-01.com.diskless:windows10

:ubuntu
kernel http://\${server_ip}/vmlinuz
initrd http://\${server_ip}/initrd.img
boot

:shell
shell

:reboot
reboot
`;

const BootScriptStep = ({ onSubmit, isSubmitting }) => {
  const { fetchServiceConfig } = useServiceManager();
  const [script, setScript] = useState(defaultScript);

  useEffect(() => {
    fetchServiceConfig("tftp-autoexec").then((script) => {
      setScript(script.text);
    });
  }, [fetchServiceConfig]);

  return (
    <div className="space-y-6">
      <div className="text-center space-y-2">
        <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-primary/10 text-primary mb-2">
          <Code className="w-6 h-6" />
        </div>
        <h2 className="text-2xl font-bold">Boot IPXE Script</h2>
        <p className="text-base-content/60 max-w-sm mx-auto">
          Customize your iPXE boot script. This script controls the initial boot
          menu and how clients load their OS.
        </p>
      </div>

      <div className="space-y-4">
        <textarea
          className="w-full h-80 p-4 font-mono text-sm bg-base-300 rounded-lg outline-none focus:ring-2 focus:ring-primary/50 transition-all resize-none"
          value={script}
          onChange={(e) => setScript(e.target.value)}
          spellCheck={false}
          disabled={isSubmitting}
        />

        <Button
          onClick={() => onSubmit(script)}
          variant="primary"
          className="w-full"
          loading={isSubmitting}
        >
          {isSubmitting
            ? "Saving Boot Script..."
            : "Save & Finish Configuration"}
        </Button>
      </div>
    </div>
  );
};

export default BootScriptStep;
