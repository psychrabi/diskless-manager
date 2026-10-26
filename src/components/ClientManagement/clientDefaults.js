export function getNextClientIdentity(clients) {
  const sortedClients = [...clients].sort((a, b) => a.name.localeCompare(b.name));
  const lastClient = sortedClients.at(-1);
  const usedNames = new Set(clients.map((client) => client.name.trim().toLowerCase()));
  const usedIps = new Set(clients.map((client) => client.ip));

  const nameMatch = lastClient?.name.match(/^(.*?)(\d+)$/);
  const prefix = nameMatch?.[1] ?? "PC";
  let nextNumber = Number(nameMatch?.[2] ?? 0) + 1;
  let name = `${prefix}${String(nextNumber).padStart(3, "0")}`;
  while (usedNames.has(name.toLowerCase())) {
    nextNumber += 1;
    name = `${prefix}${String(nextNumber).padStart(3, "0")}`;
  }

  let subnet = "192.168.1";
  let startOctet = 101;
  const ipMatch = lastClient?.ip.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
  if (ipMatch && ipMatch.slice(1).every((part) => Number(part) <= 255)) {
    subnet = ipMatch.slice(1, 4).join(".");
    const lastOctet = Number(ipMatch[4]);
    if (lastOctet >= 2 && lastOctet <= 254) {
      startOctet = lastOctet === 254 ? 2 : lastOctet + 1;
    }
  }

  let ip = "";
  for (let offset = 0; offset < 253; offset += 1) {
    const octet = ((startOctet - 2 + offset) % 253) + 2;
    const candidate = `${subnet}.${octet}`;
    if (!usedIps.has(candidate)) {
      ip = candidate;
      break;
    }
  }

  return { name, ip };
}
