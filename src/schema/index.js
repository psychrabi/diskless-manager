import z from "zod";

export const dhcpSchema = z.object({
  enabled: z.boolean(),
  subnet_ip: z.ipv4(),
  start_ip: z.ipv4(),
  end_ip: z.ipv4(),
  subnet_mask: z.ipv4(),
  gateway_ip: z.ipv4(),
  dns_server1: z.ipv4(),
  dns_server2: z.ipv4(),
  broadcast_ip: z.ipv4(),
  next_server_ip: z.ipv4(),
  boot_server_ip: z.ipv4(),
  boot_script: z.string(),
  boot_file_legacy: z.string(),
  boot_file_uefi32: z.string(),
  boot_file_uefi64: z.string(),
});

export const tftpSchema = z.object({
  enabled: z.boolean(),
  root_dir: z.string().min(1, "TFTP Root directory is required"),
  server_ip: z.string().min(1, "TFTP Server IP is required"),
  port: z.coerce.number().min(1, "TFTP Port is required"),
  options: z.string().min(1, "TFTP Options are required"),
});

export const httpSchema = z.object({
  enabled: z.boolean(),
  root_dir: z.string().min(1, "HTTP Root directory is required"),
  server_ip: z.string().min(1, "HTTP Server IP is required"),
  port: z.coerce.number().min(1, "HTTP Port is required"),
});

export const sambaSchema = z.object({
  enabled: z.boolean(),
  guest_ok: z.boolean(),
  read_only: z.boolean(),
  share_name: z.string().min(1, "Share name is required"),
  share_path: z.string().min(1, "Share path is required"),
  workgroup: z.string().min(1, "Workgroup is required"),
});

export const clientSchema = z.object({
  name: z.string().min(1, "Client name is required"),
  mac: z.mac(),
  ip: z.ipv4(),
  master: z.string(),
  snapshot: z.string().optional().nullable(),
  keep_writeback: z.boolean().default(false),
  use_game_disk: z.boolean().default(false),
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
