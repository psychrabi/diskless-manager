/**
 * Whether a client is the super client.
 *
 * The assignment decides: a client assigned an image directly, with no
 * snapshot clone, writes into that image, which makes it the machine used to
 * install drivers and capture the next revision. The stored `mode` flag is
 * only a secondary signal, so legacy or imported rows are recognized too.
 */
export function isSuperClient(client) {
  if (!client) return false;
  if (client.mode === "super") return true;
  const master = (client.master || "").trim();
  return master !== "" && master.toLowerCase() !== "pending" && !(client.snapshot || "").trim();
}
