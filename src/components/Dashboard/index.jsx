import { useShallow } from 'zustand/react/shallow';
import { useAppStore } from "@/store/useAppStore";
import { Folder, Globe, RefreshCw, Save, Server, Settings } from "lucide-react";
import React from "react";
import { Button, Card } from "../ui";

export default function Dashboard() {
    const { services, fetchServices, dependencies, serverInfo } = useAppStore(
        useShallow(state => ({
            services: state.services,
            fetchServices: state.fetchServices,
            dependencies: state.dependencies,
            serverInfo: state.serverInfo
        }))
    )
    const serverStatus = [];

    function getServiceIcon(name) {
        const icons = {
            "isc-dhcp-server": Globe,
            "tftpd-hpa": Folder,
            target: Save,
            "nfs-kernel-server": Folder,
            "smbd": Folder,
            "apache2": Globe,
        };
        return icons[name] || Settings;
    }

    return (
        <Card title="Dashboard" icon={Server} subtitle="Diskless Server Management" className='bg-base-300'>

            {/* Stats Cards */}
            <div className="stats shadow w-full bg-base-100 mb-4">
                <div className="stat">
                    <div className="stat-figure text-primary">
                        <div className="w-12 h-12 bg-primary/10 rounded-xl flex items-center justify-center text-2xl">
                            ⚙️
                        </div>
                    </div>
                    <div className="stat-title">Services</div>
                    <div className="stat-value text-primary">
                        {serverStatus?.services_running ?? 0}
                    </div>
                    <div className="stat-desc">
                        running out of {serverStatus?.services_total ?? 0} total
                    </div>
                </div>

                <div className="stat">
                    <div className="stat-figure text-secondary">
                        <div className="w-12 h-12 bg-secondary/10 rounded-xl flex items-center justify-center text-2xl">
                            💻
                        </div>
                    </div>
                    <div className="stat-title">Clients</div>
                    <div className="stat-value text-secondary">
                        {serverStatus?.clients_count ?? 0}
                    </div>
                    <div className="stat-desc">Registered boot clients</div>
                </div>

                <div className="stat">
                    <div className="stat-figure text-accent">
                        <div className="w-12 h-12 bg-accent/10 rounded-xl flex items-center justify-center text-2xl">
                            💿
                        </div>
                    </div>
                    <div className="stat-title">Images</div>
                    <div className="stat-value text-accent">
                        {serverStatus?.images_count ?? 0}
                    </div>
                    <div className="stat-desc">Available boot images</div>
                </div>

                <div className="stat">
                    <div className="stat-figure text-success">
                        <div className="w-12 h-12 bg-success/10 rounded-xl flex items-center justify-center text-2xl">
                            🧠
                        </div>
                    </div>
                    <div className="stat-title">Memory</div>
                    <div className="stat-value text-success text-2xl">
                        {serverInfo?.memory_available ?? "N/A"}
                    </div>
                    <div className="stat-desc">Available system memory</div>
                </div>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4">

                {/* System Information */}
                <div className="card bg-base-100 shadow-xl border border-base-200/50">
                    <div className="card-body p-6">
                        <h2 className="card-title text-xl mb-4">System Information</h2>
                        {serverInfo ? (
                            <div className="space-y-4">
                                <div className="flex justify-between border-b border-base-200 pb-2">
                                    <span className="text-base-content/70">Hostname</span>
                                    <span className="font-medium font-mono">
                                        {serverInfo.hostname}
                                    </span>
                                </div>
                                <div className="flex justify-between border-b border-base-200 pb-2">
                                    <span className="text-base-content/70">
                                        Operating System
                                    </span>
                                    <span className="font-medium">{serverInfo.os}</span>
                                </div>
                                <div className="flex justify-between border-b border-base-200 pb-2">
                                    <span className="text-base-content/70">Kernel</span>
                                    <span className="font-medium font-mono text-sm">
                                        {serverInfo.kernel}
                                    </span>
                                </div>
                                <div className="flex justify-between border-b border-base-200 pb-2">
                                    <span className="text-base-content/70">Uptime</span>
                                    <span className="font-medium">{serverInfo.uptime}</span>
                                </div>
                                <div className="flex justify-between border-b border-base-200 pb-2">
                                    <span className="text-base-content/70">
                                        CPU Cores
                                    </span>
                                    <span className="font-medium">
                                        {serverInfo.cpu_count}
                                    </span>
                                </div>
                                <div className="flex justify-between border-b border-base-200 pb-2">
                                    <span className="text-base-content/70">
                                        Total Memory
                                    </span>
                                    <span className="font-medium">
                                        {serverInfo.memory_total}
                                    </span>
                                </div>
                            </div>
                        ) : (
                            <div className="flex flex-col items-center justify-center h-48 text-base-content/50">
                                <span className="loading loading-dots loading-md mb-2"></span>
                                <p>System information unavailable</p>
                            </div>
                        )}
                    </div>
                </div>

                {/* Services Status */}
                <Card title="Services Status" className="col-span-2" actions={
                    <Button variant="ghost" size="icon" icon={RefreshCw} onClick={() => fetchServices()} title="Refresh all services" />
                }>
                    <div className="">
                        {services.length > 0 ? (
                            <ul className="grid grid-cols-1 md:grid-cols-2 gap-4">
                                {services.map((service) => (
                                    <li
                                        key={service.name}
                                        className="flex items-center justify-between p-3 bg-base-200 rounded-lg hover:bg-base-200 transition-colors"
                                    >
                                        <div className="flex items-center gap-3">
                                            <span className="text-xl opacity-80">
                                                {React.createElement(getServiceIcon(service.name))}
                                            </span>
                                            <div>
                                                <p className="font-medium text-base-content">
                                                    {service.display_name}
                                                </p>
                                                <p className="text-xs text-base-content/60 font-mono">
                                                    {service.name}
                                                </p>
                                            </div>
                                        </div>
                                        <span
                                            className={`badge ${service.running
                                                ? "badge-success"
                                                : "badge-error"
                                                } badge-sm font-semibold`}
                                        >
                                            {service.running ? "Running" : "Stopped"}
                                        </span>
                                    </li>
                                ))}
                            </ul>
                        ) : (
                            <div className="flex flex-col items-center justify-center h-48 text-base-content/50">
                                <p>No services found</p>
                            </div>
                        )}
                    </div>
                </Card>

            </div>



            {/* Dependencies */}
            <div className="card bg-base-100 shadow-xl border border-base-200/50">
                <div className="card-body p-6">
                    <h2 className="card-title text-xl mb-4">System Dependencies</h2>
                    <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-4">
                        {dependencies.map((dep, index) => (
                            <div
                                key={index}
                                className={`p-4 rounded-xl border ${dep.installed
                                    ? "border-success/30 bg-success/5"
                                    : "border-error/30 bg-error/5"
                                    } flex flex-col gap-2`}
                            >
                                <div className="flex items-center gap-2 mb-1">
                                    <span className="text-lg">
                                        {dep.installed ? "✅" : "❌"}
                                    </span>
                                    <span className="font-medium text-base-content">
                                        {dep.name}
                                    </span>
                                </div>
                                {dep.installed && dep.version ? (
                                    <div
                                        className="badge badge-sm badge-ghost opacity-70"
                                        title={dep.version}
                                    >
                                        {dep.version}
                                    </div>
                                ) : !dep.installed ? (
                                    <div className="badge badge-sm badge-error">
                                        Not installed
                                    </div>
                                ) : null}
                            </div>
                        ))}
                    </div>
                </div>
            </div>



        </Card >
    );
}
