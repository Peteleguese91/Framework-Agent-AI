import { describe, expect, it } from "vitest";

import { BRAND } from "./branding";

describe("centralized branding", () => {
  it("exposes a non-empty product identity", () => {
    expect(BRAND.name).toBe("GravityForge");
    expect(BRAND.shortName.length).toBeGreaterThan(0);
    expect(BRAND.tagline.length).toBeGreaterThan(0);
  });
});
