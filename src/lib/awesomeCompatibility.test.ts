import { describe, expect, it } from "vitest";
import { isAwesomeCompatibilityWarning } from "./awesomeCompatibility";

describe("Awesome Balatro source status", () => {
  it.each([
    "fully functional smods v1.0.0",
    "not mobile compatible smods v1.0.0",
    "use Smods smods v1.0.0",
    "Unrecognized source status",
  ])("keeps %s informational on desktop", (label) => {
    expect(isAwesomeCompatibilityWarning({ label, url: "" })).toBe(false);
  });

  it.each([
    "experimental smods v1.0.0",
    "partial mod conflict smods v1.0.0",
    "occasional crashes smods v0.9.8",
    "patch in works",
    "needs api rewrite smods v0.7.2",
    "buggy smods v0.9.8",
    "playable but laggy smods v1.0.0",
    "Fully functional, but occasional crashes",
  ])("flags the reported problem in %s", (label) => {
    expect(isAwesomeCompatibilityWarning({ label, url: "" })).toBe(true);
  });
});
