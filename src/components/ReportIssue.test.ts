import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ReportIssue from "./ReportIssue.svelte";
import { reportIssueStore } from "../stores/modStore";

const invokeMock = vi.hoisted(() => vi.fn());
const openExternalMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("$lib/opener", () => ({
  openExternal: (...args: unknown[]) => openExternalMock(...args),
}));

vi.mock("$lib/stores", () => ({
  addMessage: vi.fn(),
}));

if (!("animate" in Element.prototype)) {
  Object.defineProperty(Element.prototype, "animate", {
    value: () => ({
      cancel: () => {},
      play: () => {},
      pause: () => {},
      finished: Promise.resolve(),
    }),
    writable: true,
    configurable: true,
  });
}

describe("ReportIssue", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    reportIssueStore.set({ visible: false });
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_app_version") return "0.5.0";
      if (command === "submit_report") {
        return "https://github.com/jatrou/balatro-mod-manager/issues/new?title=Crash";
      }
      return null;
    });
  });

  it("opens a reviewable GitHub issue draft without claiming to upload diagnostics", async () => {
    render(ReportIssue);
    await fireEvent.click(
      screen.getByRole("button", { name: /report an issue/i }),
    );

    expect(
      screen.getByText(
        /no logs or hardware details are attached automatically/i,
      ),
    ).toBeTruthy();

    await fireEvent.input(screen.getByLabelText("Title"), {
      target: { value: "Crash" },
    });
    await fireEvent.input(screen.getByLabelText("Description"), {
      target: { value: "Crashes after updating Lovely" },
    });
    await fireEvent.click(
      screen.getByRole("button", { name: /review on github/i }),
    );

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("submit_report", {
        title: "Crash",
        description: "Crashes after updating Lovely",
        mmVersion: "0.5.0",
      });
      expect(openExternalMock).toHaveBeenCalledWith(
        "https://github.com/jatrou/balatro-mod-manager/issues/new?title=Crash",
      );
    });
  });
});
