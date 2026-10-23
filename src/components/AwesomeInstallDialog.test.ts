import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/svelte";
import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import type { AwesomeEntry } from "$lib/awesomeCatalog";
import type { AwesomeInstallSource } from "$lib/awesomeInstall";
import AwesomeInstallDialog from "./AwesomeInstallDialog.svelte";

const { invokeMock, choosePathMock, openExternalMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  choosePathMock: vi.fn(),
  openExternalMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: choosePathMock }));
vi.mock("$lib/opener", () => ({ openExternal: openExternalMock }));

const entry: AwesomeEntry = {
  id: "test-mod",
  title: "Test Mod",
  description: "",
  author: "",
  sourceUrl: "https://example.com/test.zip",
  links: [],
  categories: [],
  loader: "Steamodded",
  kind: "mod",
  badges: [],
  sourceLines: [1],
};

const download: AwesomeInstallSource = {
  kind: "download",
  source: "direct",
  url: entry.sourceUrl,
};

const prepared = {
  token: "stage-one",
  destination: "C:\\Balatro\\Mods\\Test Mod",
  warnings: [] as { code: string; message: string; blocking: boolean }[],
  mods: [{ id: "test", name: "Test Mod", version: "1.0" }],
};

function mount(
  source: AwesomeInstallSource = download,
  overrides: Partial<AwesomeEntry> = {},
) {
  const onClose = vi.fn();
  const onInstalled = vi.fn();
  render(AwesomeInstallDialog, {
    props: {
      entry: { ...entry, ...overrides },
      source,
      folderName: "Test Mod",
      onClose,
      onInstalled,
    },
  });
  return { onClose, onInstalled };
}

describe("AwesomeInstallDialog", () => {
  beforeAll(() => {
    Object.defineProperty(HTMLDialogElement.prototype, "showModal", {
      configurable: true,
      value() {
        this.open = true;
      },
    });
  });

  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    choosePathMock.mockReset();
    openExternalMock.mockReset();
  });

  afterEach(() => cleanup());

  it("prepares a direct download, requires warning acknowledgement, then installs", async () => {
    invokeMock
      .mockResolvedValueOnce({
        ...prepared,
        warnings: [
          {
            code: "conflict",
            message: "Conflicts with another mod",
            blocking: false,
          },
        ],
      })
      .mockResolvedValueOnce(prepared.destination);
    const { onInstalled } = mount();

    await waitFor(() =>
      expect(screen.getByText(prepared.destination)).toBeTruthy(),
    );
    expect(invokeMock).toHaveBeenCalledWith("prepare_awesome_install", {
      title: "Test Mod",
      folderName: "Test Mod",
      loader: "Steamodded",
      url: entry.sourceUrl,
      localPath: null,
    });
    expect(screen.getByText("Conflicts with another mod")).toBeTruthy();
    const install = screen.getByRole("button", {
      name: "Install mod",
    }) as HTMLButtonElement;
    expect(install.disabled).toBe(true);

    await fireEvent.click(
      screen.getByRole("checkbox", { name: /reviewed these warnings/i }),
    );
    expect(install.disabled).toBe(false);
    await fireEvent.click(install);
    await waitFor(() =>
      expect(onInstalled).toHaveBeenCalledWith(prepared.destination),
    );
    expect(invokeMock).toHaveBeenCalledWith("finish_awesome_install", {
      token: prepared.token,
      acceptWarnings: true,
    });
  });

  it("does not allow a blocking conflict to be acknowledged or installed", async () => {
    invokeMock.mockResolvedValueOnce({
      ...prepared,
      warnings: [
        { code: "unsafe", message: "Unsafe package layout", blocking: true },
      ],
    });
    mount();

    await waitFor(() =>
      expect(screen.getByText("Unsafe package layout")).toBeTruthy(),
    );
    expect(
      (screen.getByRole("button", { name: "Install mod" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(
      screen.queryByRole("checkbox", { name: /reviewed these warnings/i }),
    ).toBeNull();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "finish_awesome_install",
      expect.anything(),
    );
  });

  it("cancels a prepared stage without installing", async () => {
    invokeMock.mockResolvedValueOnce(prepared).mockResolvedValueOnce(undefined);
    const { onClose } = mount();
    await waitFor(() =>
      expect(screen.getByText(prepared.destination)).toBeTruthy(),
    );

    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
    expect(invokeMock).toHaveBeenCalledWith("cancel_awesome_install", {
      token: prepared.token,
    });
    expect(invokeMock).not.toHaveBeenCalledWith(
      "finish_awesome_install",
      expect.anything(),
    );
  });

  it("opens a Discord source and checks a selected local file; picker cancellation changes nothing", async () => {
    const discordUrl = "https://discord.com/channels/1/2";
    choosePathMock
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce("C:\\Downloads\\mod.lua");
    invokeMock.mockResolvedValueOnce(prepared);
    mount({
      kind: "local",
      sourceUrl: discordUrl,
      discordUrl,
      reason: "Download the file from Discord.",
    });

    await waitFor(() =>
      expect(openExternalMock).toHaveBeenCalledWith(discordUrl),
    );
    const path = screen.getByRole("textbox", {
      name: "Downloaded file or folder location",
    }) as HTMLInputElement;
    await fireEvent.click(screen.getByRole("button", { name: "Browse files" }));
    expect(path.value).toBe("");
    expect(
      (
        screen.getByRole("button", {
          name: "Check download",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: "Browse files" }));
    expect(path.value).toBe("C:\\Downloads\\mod.lua");
    await fireEvent.click(
      screen.getByRole("button", { name: "Check download" }),
    );
    await waitFor(() =>
      expect(screen.getByText(prepared.destination)).toBeTruthy(),
    );
    expect(invokeMock).toHaveBeenCalledWith("prepare_awesome_install", {
      title: "Test Mod",
      folderName: "Test Mod",
      loader: "Steamodded",
      url: null,
      localPath: "C:\\Downloads\\mod.lua",
    });
  });

  it("shows local import when a direct download cannot be prepared", async () => {
    invokeMock.mockRejectedValueOnce("HTTP 404");
    mount();
    await waitFor(() => expect(screen.getByText("HTTP 404")).toBeTruthy());
    expect(
      screen.getByRole("textbox", {
        name: "Downloaded file or folder location",
      }),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Retry download" })).toBeTruthy();
  });
});
