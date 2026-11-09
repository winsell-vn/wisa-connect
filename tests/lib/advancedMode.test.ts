import { beforeEach, describe, expect, it } from "vitest";
import { readAdvancedMode, storeAdvancedMode } from "@/lib/advancedMode";

/**
 * The inherited cc-switch surface (providers, proxy, MCP…) must stay hidden
 * until someone asks for it. A default that flipped to "on" would put every
 * user one wrong click away from hand-editing the config WISA Connect just
 * wrote for them, so the default is worth a test.
 */
describe("advanced mode", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("is off on a fresh machine", () => {
    expect(readAdvancedMode()).toBe(false);
  });

  it("remembers being turned on", () => {
    storeAdvancedMode(true);
    expect(readAdvancedMode()).toBe(true);
  });

  it("remembers being turned back off", () => {
    storeAdvancedMode(true);
    storeAdvancedMode(false);
    expect(readAdvancedMode()).toBe(false);
  });

  it("treats an unreadable value as off", () => {
    localStorage.setItem("wisa-connect-advanced-mode", "yes");
    expect(readAdvancedMode()).toBe(false);
  });
});
