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
  mac: z
    .string()
    .regex(
      /^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/,
      "Invalid MAC address format"
    ),
  ip: z
    .string()
    .regex(
      /^([\d]{1,3}\.){3}\d{1,3}$/,
      "Invalid IP address format. Use X.X.X.X"
    ),
  master: z.string().optional(),
  snapshot: z.string().optional().nullable(),
  keep_writeback: z.boolean().optional(),
  use_game_disk: z.boolean().optional(),
});
