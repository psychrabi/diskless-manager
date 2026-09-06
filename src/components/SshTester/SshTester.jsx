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
    <div className={`alert ${result.success ? "alert-success" : "alert-error"} mt-4`}>
      <div className="w-full">
        <h3 className="font-bold">{title}</h3>
        <p>{result.message}</p>
        <p className="text-sm opacity-70">Duration: {result.duration_ms}ms</p>
        {result.command_output && (
          <div className="mt-2">
            {outputTitle && <h4 className="font-semibold">{outputTitle}</h4>}
            <pre className="text-xs bg-base-200 p-3 rounded overflow-x-auto whitespace-pre-wrap">
              {result.command_output}
            </pre>
          </div>
        )}
      </div>
    </div>
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

      <div className="card bg-base-100 shadow-xl mb-6">
        <div className="card-body">
          <h2 className="card-title">Test SSH Connection</h2>

          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <div className="form-control">
              <label className="label">
                <span className="label-text">Host/IP Address</span>
              </label>
              <input
                type="text"
                placeholder="192.168.1.100"
                className="input input-bordered"
                value={connectionForm.host}
                onChange={(e) =>
                  setConnectionForm({ ...connectionForm, host: e.target.value })
                }
              />
            </div>

            <div className="form-control">
              <label className="label">
                <span className="label-text">Username</span>
              </label>
              <input
                type="text"
                placeholder={DEFAULT_USERNAME}
                className="input input-bordered"
                value={connectionForm.username}
                onChange={(e) =>
                  setConnectionForm({
                    ...connectionForm,
                    username: e.target.value,
                  })
                }
              />
            </div>

            <div className="form-control">
              <label className="label">
                <span className="label-text">Password</span>
              </label>
              <input
                type="password"
                placeholder="Leave blank for key auth"
                className="input input-bordered"
                value={connectionForm.password}
                onChange={(e) =>
                  setConnectionForm({
                    ...connectionForm,
                    password: e.target.value,
                  })
                }
              />
            </div>

            <div className="form-control">
              <label className="label">
                <span className="label-text">Port</span>
              </label>
              <input
                type="number"
                placeholder="22"
                className="input input-bordered"
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

          <div className="card-actions justify-end mt-4">
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
        </div>
      </div>

      {systemInfo && (
        <div className="card bg-base-100 shadow-xl mb-6">
          <div className="card-body">
            <h2 className="card-title">Windows System Information</h2>

            {systemInfo.error ? (
              <div className="alert alert-error">
                <p>{systemInfo.error}</p>
              </div>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="stat">
                  <div className="stat-title">Computer Name</div>
                  <div className="stat-value text-lg">
                    {systemInfo.computer_name}
                  </div>
                </div>
                <div className="stat">
                  <div className="stat-title">OS Version</div>
                  <div className="stat-value text-lg">{systemInfo.os_version}</div>
                </div>
                <div className="stat">
                  <div className="stat-title">Architecture</div>
                  <div className="stat-value text-lg">
                    {systemInfo.architecture}
                  </div>
                </div>
                <div className="stat">
                  <div className="stat-title">Total Memory</div>
                  <div className="stat-value text-lg">{systemInfo.total_memory}</div>
                </div>
                <div className="stat">
                  <div className="stat-title">Available Memory</div>
                  <div className="stat-value text-lg">
                    {systemInfo.available_memory}
                  </div>
                </div>
                <div className="stat">
                  <div className="stat-title">CPU</div>
                  <div className="stat-value text-sm">{systemInfo.cpu_info}</div>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      <div className="card bg-base-100 shadow-xl">
        <div className="card-body">
          <h2 className="card-title">Execute SSH Command</h2>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="form-control">
              <label className="label">
                <span className="label-text">Host/IP Address</span>
              </label>
              <input
                type="text"
                placeholder="192.168.1.100"
                className="input input-bordered"
                value={commandForm.host}
                onChange={(e) =>
                  setCommandForm({ ...commandForm, host: e.target.value })
                }
              />
            </div>

            <div className="form-control">
              <label className="label">
                <span className="label-text">Username</span>
              </label>
              <input
                type="text"
                placeholder={DEFAULT_USERNAME}
                className="input input-bordered"
                value={commandForm.username}
                onChange={(e) =>
                  setCommandForm({
                    ...commandForm,
                    username: e.target.value,
                  })
                }
              />
            </div>

            <div className="form-control">
              <label className="label">
                <span className="label-text">Password</span>
              </label>
              <input
                type="password"
                placeholder="Leave blank for key auth"
                className="input input-bordered"
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

          <div className="form-control">
            <label className="label">
              <span className="label-text">Command</span>
            </label>
            <textarea
              className="textarea textarea-bordered h-24"
              placeholder="Enter command to execute..."
              value={commandForm.command}
              onChange={(e) =>
                setCommandForm({ ...commandForm, command: e.target.value })
              }
            />
            <div className="label">
              <span className="label-text-alt">
                Examples: dir, ipconfig, systeminfo, Get-Process
              </span>
            </div>
          </div>

          <div className="card-actions justify-end">
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
        </div>
      </div>

      <div className="card bg-base-100 shadow-xl mt-6">
        <div className="card-body">
          <h2 className="card-title">Quick Commands</h2>
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
        </div>
      </div>
    </div>
  );
};

export default SshTester;
