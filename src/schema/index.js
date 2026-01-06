import { z } from "zod";

// Helper for IP address validation
const ipSchema = z.ipv4();

// Helper for MAC address validation
const macSchema = z
  .string()
  .regex(/^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/, "Invalid MAC address");

export const dhcpSchema = z.object({
  enabled: z.boolean(),
  subnet_ip: ipSchema,
  start_ip: ipSchema,
  end_ip: ipSchema,
  subnet_mask: ipSchema,
  gateway_ip: ipSchema,
  dns_server1: ipSchema,
  dns_server2: ipSchema,
  broadcast_ip: ipSchema,
  next_server_ip: ipSchema,
  boot_server_ip: ipSchema,
  boot_script: z.string(),
  boot_file_legacy: z.string(),
  boot_file_uefi32: z.string(),
  boot_file_uefi64: z.string(),
});

export const tftpSchema = z.object({
  enabled: z.boolean(),
  root_dir: z.string().min(1, "TFTP Root directory is required"),
  server_ip: ipSchema,
  port: z.coerce.number().min(1, "TFTP Port is required"),
  options: z.string().min(1, "TFTP Options are required"),
});

export const httpSchema = z.object({
  enabled: z.boolean(),
  root_dir: z.string().min(1, "HTTP Root directory is required"),
  server_ip: ipSchema,
  port: z.coerce.number().min(1, "HTTP Port is required"),
});

export const iscsiSchema = z.object({
  enabled: z.boolean(),
  target_prefix: z.string().min(1, "Target prefix is required"),
  portal_port: z.coerce.number().min(1, "Portal port is required"),
  targets_dir: z.string().min(1, "Targets directory is required"),
});

export const sambaSchema = z.object({
  enabled: z.boolean(),
  guest_ok: z.boolean(),
  read_only: z.boolean(),
  share_name: z.string().min(1, "Share name is required"),
  share_path: z.string().min(1, "Share path is required"),
  workgroup: z.string().min(1, "Workgroup is required"),
});

export const serverSchema = z.object({
  interface: z.array(z.string()).min(1, "Select at least one interface"),
  ip_address: ipSchema,
  netmask: ipSchema,
  gateway: ipSchema,
  dns: z.string().refine(
    (val) =>
      val
        .split(",")
        .map((s) => s.trim())
        .every((ip) => z.ipv4().safeParse(ip).success),
    "Must be a comma-separated list of valid IPv4 addresses"
  ),
  hostname: z.string().min(1, "Hostname is required"),
  domain: z.string().min(1, "Domain is required"),
});

export const clientSchema = z.object({
  name: z.string().min(1, "Client name is required"),
  mac: macSchema,
  ip: ipSchema,
  master: z.string().min(1, "Image selection is required"),
  snapshot: z.string().optional().nullable(),
  keep_writeback: z.boolean().default(false),
  use_game_disk: z.boolean().default(false),
  enabled: z.boolean().default(true),
});

export const imageSchema = z.object({
  name: z.string().min(1, "Image name is required"),
  os_type: z.enum(["linux", "windows"]).default("windows"),
  size_gb: z.coerce.number().min(1, "Image Size is required"),
  format: z.enum(["raw", "qcow2"]).default("raw"),
  description: z.string().optional(),
});

export const cloneSchema = z.object({
  image_id: z.string().min(1, "Image ID is required"),
  name: z.string().min(1, "Image name is required"),
});
