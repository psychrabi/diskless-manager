import { expect, it } from "vitest";
import { getNextClientIdentity } from "./clientDefaults";

it("skips names already in use and selects the next free valid address in the same subnet", () => {
  expect(getNextClientIdentity([
    { name: "PC001", ip: "192.168.1.2" },
    { name: "PC002", ip: "192.168.1.253" },
    { name: "PC003", ip: "192.168.1.255" },
    { name: "Zulu", ip: "192.168.1.254" },
  ])).toEqual({ name: "PC004", ip: "192.168.1.3" });
});

it("uses safe defaults when there are no clients", () => {
  expect(getNextClientIdentity([])).toEqual({ name: "PC001", ip: "192.168.1.101" });
});
