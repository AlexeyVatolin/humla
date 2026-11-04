import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { mockTauri } from "../test/tauri";

vi.hoisted(() => {
  Object.defineProperty(navigator, "userAgent", { configurable: true, value: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)" });
});

import { Permissions } from "./Permissions";
import { formatAccel, DEFAULT_RECORD_HOTKEY } from "../lib/hotkey";
import { DEFAULT_DIARIZE_ENGINE, enginesToDownload } from "../lib/diarizeEngine";

describe("Windows setup", () => {
  it("opens microphone privacy settings and refreshes access when the user returns", async () => {
    let granted = false;
    const open = vi.fn();
    mockTauri({
      permissions_status: () => ({ microphone: granted ? "granted" : "denied", screen: "not_required" }),
      permissions_open_settings: open,
    });
    render(<Permissions />);
    expect(await screen.findByText("System audio")).toBeInTheDocument();
    expect(screen.getByText("Available")).toBeInTheDocument();
    expect(screen.queryByText("Request")).not.toBeInTheDocument();
    expect(screen.queryByText(/must restart/i)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Windows Settings/ }));
    expect(open).toHaveBeenCalledWith({ kind: "microphone" });
    granted = true;
    fireEvent.focus(window);
    await waitFor(() => expect(screen.getByText("Granted")).toBeInTheDocument());
    expect(screen.queryByRole("button", { name: /Windows Settings/ })).not.toBeInTheDocument();
  });

  it("uses Windows shortcuts and downloads only the Windows speaker model", () => {
    expect(DEFAULT_RECORD_HOTKEY).toBe("Control+Alt+KeyR");
    expect(formatAccel(DEFAULT_RECORD_HOTKEY)).toBe("Ctrl+Alt+R");
    expect(DEFAULT_DIARIZE_ENGINE).toBe("windows");
    expect(enginesToDownload("windows")).toEqual(["windows"]);
  });
});
