import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { mockTauri } from "./test/tauri";

beforeEach(() => {
  vi.resetModules();
  document.body.innerHTML = '<p id="message" role="status"></p><button id="dismiss">Not now</button><button id="record">Record</button>';
});

describe("Zoom meeting prompt", () => {
  it("starts once and prevents duplicate clicks while starting", async () => {
    const start = vi.fn(() => new Promise(() => {}));
    mockTauri({ meeting_prompt_start: start });
    await import("./meetingPrompt");
    const record = screen.getByRole("button", { name: "Record" });
    fireEvent.click(record);
    expect(record).toBeDisabled();
    await waitFor(() => expect(start).toHaveBeenCalledOnce());
  });

  it("dismisses without starting a recording", async () => {
    const start = vi.fn();
    const dismiss = vi.fn();
    mockTauri({ meeting_prompt_start: start, meeting_prompt_dismiss: dismiss });
    await import("./meetingPrompt");
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    await waitFor(() => expect(dismiss).toHaveBeenCalledOnce());
    expect(start).not.toHaveBeenCalled();
  });

  it("shows a failed start and allows retrying", async () => {
    mockTauri({ meeting_prompt_start: () => { throw new Error("Microphone unavailable"); } });
    await import("./meetingPrompt");
    const record = screen.getByRole("button", { name: "Record" });
    fireEvent.click(record);
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Microphone unavailable"));
    expect(record).toBeEnabled();
  });
});
