import { Card as ShadcnCard, CardContent, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input as TextInput } from "@/components/ui/input";
import { Alert } from "@/components/ui/alert";
import { Textarea } from "@/components/ui/textarea";
import { useMemo, useState } from "react";
import { testSshConnection, executeSshCommand, getWindowsSystemInfo } from "../../api/modules/ssh";
import { Button } from "@/components/ui";

const DEFAULT_USERNAME = "Administrator";

const INITIAL_CONNECTION_FORM = {
  host: "",
  username: DEFAULT_USERNAME,
  port: 22,
  password: "",
};

const INITIAL_COMMAND_FORM = {
  host: "",
  username: DEFAULT_USERNAME,
  command: 'echo "Hello from Windows SSH"',
  password: "",
};

const QUICK_COMMANDS = [
  { label: "List C: Drive", command: "dir C:\\" },
  { label: "Network Config", command: "ipconfig /all" },
  { label: "System Info", command: "systeminfo" },
  { label: "Top Processes", command: "Get-Process | Select-Object -First 10" },
  {
    label: "Running Services",
    command:
      'Get-Service | Where-Object {$_.Status -eq "Running"} | Select-Object -First 10',
  },
];

const buildApiErrorResult = (error) => ({
  success: false,
  message: `Error: ${error?.message || error}`,
  duration_ms: 0,
  command_output: null,
});

const ResultAlert = ({ title, result, outputTitle }) => {
  if (!result) return null;

  return (
    <Alert variant={result.success ? "default" : "destructive"} className="mt-4">
      <div className="w-full">
        <h3 className="font-bold">{title}</h3>
        <p>{result.message}</p>
        <p className="text-sm opacity-70">Duration: {result.duration_ms}ms</p>
        {result.command_output && (
          <div className="mt-2">
            {outputTitle && <h4 className="font-semibold">{outputTitle}</h4>}
            <pre className="text-xs bg-muted p-3 rounded overflow-x-auto whitespace-pre-wrap">
              {result.command_output}
            </pre>
          </div>
        )}
      </div>
    </Alert>
  );
};

const SshTester = () => {
  const [connectionForm, setConnectionForm] = useState(INITIAL_CONNECTION_FORM);
  const [commandForm, setCommandForm] = useState(INITIAL_COMMAND_FORM);
  const [testResult, setTestResult] = useState(null);
  const [commandResult, setCommandResult] = useState(null);
  const [systemInfo, setSystemInfo] = useState(null);
  const [loading, setLoading] = useState(false);

  const canUseConnectionActions = useMemo(
    () => Boolean(connectionForm.host && connectionForm.username && !loading),
    [connectionForm.host, connectionForm.username, loading]
  );

  const canExecuteCommand = useMemo(
    () =>
      Boolean(
        commandForm.host && commandForm.username && commandForm.command && !loading
      ),
    [commandForm.command, commandForm.host, commandForm.username, loading]
  );

  const runWithLoading = async ({ before, action, onSuccess, onError }) => {
    setLoading(true);
    if (before) before();

    try {
      const result = await action();
      onSuccess(result);
    } catch (error) {
      onError(error);
    } finally {
      setLoading(false);
    }
  };

  const testConnection = async () => {
    await runWithLoading({
      before: () => setTestResult(null),
      action: () =>
        testSshConnection(
          connectionForm.host,
          connectionForm.username,
          connectionForm.port,
          connectionForm.password
        ),
      onSuccess: (result) => setTestResult(result),
      onError: (error) => setTestResult(buildApiErrorResult(error)),
    });
  };

  const executeCommand = async () => {
    await runWithLoading({
      before: () => setCommandResult(null),
      action: () =>
        executeSshCommand(
          commandForm.host,
          commandForm.username,
          commandForm.command,
          commandForm.password
        ),
      onSuccess: (result) => setCommandResult(result),
      onError: (error) => setCommandResult(buildApiErrorResult(error)),
    });
  };

  const getSystemInfo = async () => {
    await runWithLoading({
      before: () => setSystemInfo(null),
      action: () =>
        getWindowsSystemInfo(
          connectionForm.host,
          connectionForm.username,
          connectionForm.password
        ),
      onSuccess: (result) => setSystemInfo(result),
      onError: (error) =>
        setSystemInfo({
          error: `Error: ${error?.message || error}`,
        }),
    });
  };

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <h1 className="text-2xl font-bold mb-6">SSH Connection Tester</h1>

      <ShadcnCard className="shadow-sm mb-6">
        <CardContent className="flex flex-col gap-4">
          <CardTitle className="">Test SSH Connection</CardTitle>

          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="connectionForm-host" className="flex items-center gap-2 text-sm font-medium">
                <span className="text-sm font-medium">Host/IP Address</span>
              </Label>
              <TextInput
                type="text"
                placeholder="192.168.1.100"
                className=""
                id="connectionForm-host"
                value={connectionForm.host}
                onChange={(e) =>
                  setConnectionForm({ ...connectionForm, host: e.target.value })
                }
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="connectionForm-username" className="flex items-center gap-2 text-sm font-medium">
                <span className="text-sm font-medium">Username</span>
              </Label>
              <TextInput
                type="text"
                placeholder={DEFAULT_USERNAME}
                className=""
                id="connectionForm-username"
                value={connectionForm.username}
                onChange={(e) =>
                  setConnectionForm({
                    ...connectionForm,
                    username: e.target.value,
                  })
                }
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="connectionForm-password" className="flex items-center gap-2 text-sm font-medium">
                <span className="text-sm font-medium">Password</span>
              </Label>
              <TextInput
                type="password"
                placeholder="Leave blank for key auth"
                className=""
                id="connectionForm-password"
                value={connectionForm.password}
                onChange={(e) =>
                  setConnectionForm({
                    ...connectionForm,
                    password: e.target.value,
                  })
                }
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="connectionForm-port" className="flex items-center gap-2 text-sm font-medium">
                <span className="text-sm font-medium">Port</span>
              </Label>
              <TextInput
                type="number"
                placeholder="22"
                className=""
                id="connectionForm-port"
                value={connectionForm.port}
                onChange={(e) =>
                  setConnectionForm({
                    ...connectionForm,
                    port: Number.parseInt(e.target.value, 10) || 22,
                  })
                }
              />
            </div>
          </div>

          <div className="flex flex-wrap gap-2 justify-end mt-4">
            <Button
              variant="primary"
              loading={loading}
              onClick={testConnection}
              disabled={!canUseConnectionActions}
            >
              Test Connection
            </Button>
            <Button
              variant="secondary"
              loading={loading}
              onClick={getSystemInfo}
              disabled={!canUseConnectionActions}
            >
              Get System Info
            </Button>
          </div>

          <ResultAlert title="Connection Test Result" result={testResult} />
        </CardContent>
      </ShadcnCard>

      {systemInfo && (
        <ShadcnCard className="shadow-sm mb-6">
          <CardContent className="flex flex-col gap-4">
            <CardTitle className="">Windows System Information</CardTitle>

            {systemInfo.error ? (
              <Alert variant="destructive" className="">
                <p>{systemInfo.error}</p>
              </Alert>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="space-y-1 rounded-lg border p-4">
                  <div className="text-sm text-muted-foreground">Computer Name</div>
                  <div className="font-semibold text-lg">
                    {systemInfo.computer_name}
                  </div>
                </div>
                <div className="space-y-1 rounded-lg border p-4">
                  <div className="text-sm text-muted-foreground">OS Version</div>
                  <div className="font-semibold text-lg">{systemInfo.os_version}</div>
                </div>
                <div className="space-y-1 rounded-lg border p-4">
                  <div className="text-sm text-muted-foreground">Architecture</div>
                  <div className="font-semibold text-lg">
                    {systemInfo.architecture}
                  </div>
                </div>
                <div className="space-y-1 rounded-lg border p-4">
                  <div className="text-sm text-muted-foreground">Total Memory</div>
                  <div className="font-semibold text-lg">{systemInfo.total_memory}</div>
                </div>
                <div className="space-y-1 rounded-lg border p-4">
                  <div className="text-sm text-muted-foreground">Available Memory</div>
                  <div className="font-semibold text-lg">
                    {systemInfo.available_memory}
                  </div>
                </div>
                <div className="space-y-1 rounded-lg border p-4">
                  <div className="text-sm text-muted-foreground">CPU</div>
                  <div className="font-semibold text-sm">{systemInfo.cpu_info}</div>
                </div>
              </div>
            )}
          </CardContent>
        </ShadcnCard>
      )}

      <ShadcnCard className="shadow-sm">
        <CardContent className="flex flex-col gap-4">
          <CardTitle className="">Execute SSH Command</CardTitle>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="commandForm-host" className="flex items-center gap-2 text-sm font-medium">
                <span className="text-sm font-medium">Host/IP Address</span>
              </Label>
              <TextInput
                type="text"
                placeholder="192.168.1.100"
                className=""
                id="commandForm-host"
                value={commandForm.host}
                onChange={(e) =>
                  setCommandForm({ ...commandForm, host: e.target.value })
                }
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="commandForm-username" className="flex items-center gap-2 text-sm font-medium">
                <span className="text-sm font-medium">Username</span>
              </Label>
              <TextInput
                type="text"
                placeholder={DEFAULT_USERNAME}
                className=""
                id="commandForm-username"
                value={commandForm.username}
                onChange={(e) =>
                  setCommandForm({
                    ...commandForm,
                    username: e.target.value,
                  })
                }
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="commandForm-password" className="flex items-center gap-2 text-sm font-medium">
                <span className="text-sm font-medium">Password</span>
              </Label>
              <TextInput
                type="password"
                placeholder="Leave blank for key auth"
                className=""
                id="commandForm-password"
                value={commandForm.password}
                onChange={(e) =>
                  setCommandForm({
                    ...commandForm,
                    password: e.target.value,
                  })
                }
              />
            </div>
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="ssh-command" className="flex items-center gap-2 text-sm font-medium">
              <span className="text-sm font-medium">Command</span>
            </Label>
            <Textarea
              id="ssh-command"
              className="h-24"
              placeholder="Enter command to execute..."
              value={commandForm.command}
              onChange={(e) =>
                setCommandForm({ ...commandForm, command: e.target.value })
              }
            />
            <div className="flex items-center gap-2 text-sm font-medium">
              <span className="text-xs text-muted-foreground">
                Examples: dir, ipconfig, systeminfo, Get-Process
              </span>
            </div>
          </div>

          <div className="flex flex-wrap gap-2 justify-end">
            <Button
              variant="primary"
              loading={loading}
              onClick={executeCommand}
              disabled={!canExecuteCommand}
            >
              Execute Command
            </Button>
          </div>

          <ResultAlert
            title="Command Execution Result"
            result={commandResult}
            outputTitle="Output:"
          />
        </CardContent>
      </ShadcnCard>

      <ShadcnCard className="shadow-sm mt-6">
        <CardContent className="flex flex-col gap-4">
          <CardTitle className="">Quick Commands</CardTitle>
          <div className="flex flex-wrap gap-2">
            {QUICK_COMMANDS.map((entry) => (
              <Button
                key={entry.label}
                size="sm"
                variant="outline"
                onClick={() =>
                  setCommandForm({
                    ...commandForm,
                    command: entry.command,
                  })
                }
              >
                {entry.label}
              </Button>
            ))}
          </div>
        </CardContent>
      </ShadcnCard>
    </div>
  );
};

export default SshTester;
