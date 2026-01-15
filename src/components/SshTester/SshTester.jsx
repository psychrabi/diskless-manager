import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

const SshTester = () => {
  const [connectionForm, setConnectionForm] = useState({
    host: '',
    username: 'Administrator', // Default for Windows
    port: 22
  });
  
  const [commandForm, setCommandForm] = useState({
    host: '',
    username: 'Administrator',
    command: 'echo "Hello from Windows SSH"'
  });
  
  const [testResult, setTestResult] = useState(null);
  const [commandResult, setCommandResult] = useState(null);
  const [systemInfo, setSystemInfo] = useState(null);
  const [loading, setLoading] = useState(false);

  const testConnection = async () => {
    setLoading(true);
    setTestResult(null);
    
    try {
      const result = await invoke('test_ssh_connection', {
        request: {
          host: connectionForm.host,
          username: connectionForm.username,
          port: connectionForm.port
        }
      });
      setTestResult(result);
    } catch (error) {
      setTestResult({
        success: false,
        message: `Error: ${error}`,
        duration_ms: 0,
        command_output: null
      });
    } finally {
      setLoading(false);
    }
  };

  const executeCommand = async () => {
    setLoading(true);
    setCommandResult(null);
    
    try {
      const result = await invoke('execute_ssh_command', {
        host: commandForm.host,
        username: commandForm.username,
        command: commandForm.command
      });
      setCommandResult(result);
    } catch (error) {
      setCommandResult({
        success: false,
        message: `Error: ${error}`,
        duration_ms: 0,
        command_output: null
      });
    } finally {
      setLoading(false);
    }
  };

  const getSystemInfo = async () => {
    setLoading(true);
    setSystemInfo(null);
    
    try {
      const result = await invoke('get_windows_system_info', {
        host: connectionForm.host,
        username: connectionForm.username
      });
      setSystemInfo(result);
    } catch (error) {
      setSystemInfo({
        error: `Error: ${error}`
      });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <h1 className="text-2xl font-bold mb-6">SSH Connection Tester</h1>
      
      {/* Connection Test Section */}
      <div className="card bg-base-100 shadow-xl mb-6">
        <div className="card-body">
          <h2 className="card-title">Test SSH Connection</h2>
          
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="form-control">
              <label className="label">
                <span className="label-text">Host/IP Address</span>
              </label>
              <input
                type="text"
                placeholder="192.168.1.100"
                className="input input-bordered"
                value={connectionForm.host}
                onChange={(e) => setConnectionForm({...connectionForm, host: e.target.value})}
              />
            </div>
            
            <div className="form-control">
              <label className="label">
                <span className="label-text">Username</span>
              </label>
              <input
                type="text"
                placeholder="Administrator"
                className="input input-bordered"
                value={connectionForm.username}
                onChange={(e) => setConnectionForm({...connectionForm, username: e.target.value})}
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
                onChange={(e) => setConnectionForm({...connectionForm, port: parseInt(e.target.value)})}
              />
            </div>
          </div>
          
          <div className="card-actions justify-end mt-4">
            <button 
              className={`btn btn-primary ${loading ? 'loading' : ''}`}
              onClick={testConnection}
              disabled={!connectionForm.host || !connectionForm.username || loading}
            >
              Test Connection
            </button>
            <button 
              className={`btn btn-secondary ${loading ? 'loading' : ''}`}
              onClick={getSystemInfo}
              disabled={!connectionForm.host || !connectionForm.username || loading}
            >
              Get System Info
            </button>
          </div>
          
          {testResult && (
            <div className={`alert ${testResult.success ? 'alert-success' : 'alert-error'} mt-4`}>
              <div>
                <h3 className="font-bold">Connection Test Result</h3>
                <p>{testResult.message}</p>
                <p className="text-sm opacity-70">Duration: {testResult.duration_ms}ms</p>
                {testResult.command_output && (
                  <pre className="text-xs mt-2 bg-base-200 p-2 rounded">
                    {testResult.command_output}
                  </pre>
                )}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* System Info Section */}
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
                  <div className="stat-value text-lg">{systemInfo.computer_name}</div>
                </div>
                <div className="stat">
                  <div className="stat-title">OS Version</div>
                  <div className="stat-value text-lg">{systemInfo.os_version}</div>
                </div>
                <div className="stat">
                  <div className="stat-title">Architecture</div>
                  <div className="stat-value text-lg">{systemInfo.architecture}</div>
                </div>
                <div className="stat">
                  <div className="stat-title">Total Memory</div>
                  <div className="stat-value text-lg">{systemInfo.total_memory}</div>
                </div>
                <div className="stat">
                  <div className="stat-title">Available Memory</div>
                  <div className="stat-value text-lg">{systemInfo.available_memory}</div>
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

      {/* Command Execution Section */}
      <div className="card bg-base-100 shadow-xl">
        <div className="card-body">
          <h2 className="card-title">Execute SSH Command</h2>
          
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="form-control">
              <label className="label">
                <span className="label-text">Host/IP Address</span>
              </label>
              <input
                type="text"
                placeholder="192.168.1.100"
                className="input input-bordered"
                value={commandForm.host}
                onChange={(e) => setCommandForm({...commandForm, host: e.target.value})}
              />
            </div>
            
            <div className="form-control">
              <label className="label">
                <span className="label-text">Username</span>
              </label>
              <input
                type="text"
                placeholder="Administrator"
                className="input input-bordered"
                value={commandForm.username}
                onChange={(e) => setCommandForm({...commandForm, username: e.target.value})}
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
              onChange={(e) => setCommandForm({...commandForm, command: e.target.value})}
            />
            <div className="label">
              <span className="label-text-alt">Examples: dir, ipconfig, systeminfo, Get-Process</span>
            </div>
          </div>
          
          <div className="card-actions justify-end">
            <button 
              className={`btn btn-primary ${loading ? 'loading' : ''}`}
              onClick={executeCommand}
              disabled={!commandForm.host || !commandForm.username || !commandForm.command || loading}
            >
              Execute Command
            </button>
          </div>
          
          {commandResult && (
            <div className={`alert ${commandResult.success ? 'alert-success' : 'alert-error'} mt-4`}>
              <div className="w-full">
                <h3 className="font-bold">Command Execution Result</h3>
                <p>{commandResult.message}</p>
                <p className="text-sm opacity-70">Duration: {commandResult.duration_ms}ms</p>
                {commandResult.command_output && (
                  <div className="mt-2">
                    <h4 className="font-semibold">Output:</h4>
                    <pre className="text-xs bg-base-200 p-3 rounded overflow-x-auto whitespace-pre-wrap">
                      {commandResult.command_output}
                    </pre>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Quick Commands */}
      <div className="card bg-base-100 shadow-xl mt-6">
        <div className="card-body">
          <h2 className="card-title">Quick Commands</h2>
          <div className="flex flex-wrap gap-2">
            <button 
              className="btn btn-sm btn-outline"
              onClick={() => setCommandForm({...commandForm, command: 'dir C:\\'})}
            >
              List C: Drive
            </button>
            <button 
              className="btn btn-sm btn-outline"
              onClick={() => setCommandForm({...commandForm, command: 'ipconfig /all'})}
            >
              Network Config
            </button>
            <button 
              className="btn btn-sm btn-outline"
              onClick={() => setCommandForm({...commandForm, command: 'systeminfo'})}
            >
              System Info
            </button>
            <button 
              className="btn btn-sm btn-outline"
              onClick={() => setCommandForm({...commandForm, command: 'Get-Process | Select-Object -First 10'})}
            >
              Top Processes
            </button>
            <button 
              className="btn btn-sm btn-outline"
              onClick={() => setCommandForm({...commandForm, command: 'Get-Service | Where-Object {$_.Status -eq "Running"} | Select-Object -First 10'})}
            >
              Running Services
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default SshTester;