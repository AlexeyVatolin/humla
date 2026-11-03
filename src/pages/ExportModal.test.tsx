import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DISCONNECTED, useCloudStore } from "../lib/cloud";
import type { ExportSpec, Note } from "../lib/ipc";
import { useRecordingStore } from "../lib/store";
import { mockTauri } from "../test/tauri";
import { ExportModal } from "./ExportModal";

function makeNote(over: Partial<Note> = {}): Note {
  return {
    id: "n1",
    title: "Weekly Sync",
    body: "<p>Some typed notes</p>",
    transcript: "You: hello\nSpeaker 1: hi there",
    summary: "- Ship v1",
    audio_path: null,
    summary_preset: "meeting",
    folder_id: null,
    language: "en",
    summary_provider: "",
    expected_speakers: null,
    detected_language: null,
    created_at: 0,
    updated_at: 0,
    owner: "",
    workspace_id: "",
    private: false,
    ...over,
  };
}

// Wires the native save panel to a fixed path and records the export invoke.
function setup(
  over: Partial<Note> = {},
  savePath: string | null = "/Users/me/weekly-sync.md",
  flushEdits: () => Promise<void> = () => Promise.resolve(),
) {
  const exportSpy = vi.fn();
  mockTauri({
    "plugin:dialog|save": () => savePath,
    export_note: (args) => {
      exportSpy(args);
      return undefined;
    },
  });
  const onClose = vi.fn();
  const onCreateTeam = vi.fn();
  render(
    <ExportModal
      note={makeNote(over)}
      open
      onClose={onClose}
      flushEdits={flushEdits}
    />,
  );
  return { exportSpy, onClose, onCreateTeam };
}


beforeEach(() => {
  localStorage.clear();
  // Default: Personal, which is the only state the team hint appears in.
  useCloudStore.setState({ status: { ...DISCONNECTED, configured: true } });
});

describe("ExportModal", () => {
  it("defaults to Summary + Transcript checked, Notes off", () => {
    setup();
    const summary = screen.getByRole("checkbox", { name: "Summary" });
    const transcript = screen.getByRole("checkbox", { name: "Transcript" });
    const notes = screen.getByRole("checkbox", { name: "Notes" });
    expect(summary).toBeChecked();
    expect(transcript).toBeChecked();
    expect(notes).not.toBeChecked();
  });

  it("exports with the chosen path and default spec", async () => {
    const { exportSpy, onClose } = setup();
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));
    await waitFor(() => expect(exportSpy).toHaveBeenCalledTimes(1));
    expect(exportSpy).toHaveBeenCalledWith({
      noteId: "n1",
      spec: {
        path: "/Users/me/weekly-sync.md",
        format: "markdown",
        includeSummary: true,
        includeTranscript: true,
        includeNotes: false,
        includeSpeakerLabels: true,
      },
    });
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("waits for the note's queued edits to save before exporting", async () => {
    // `export_note` reads the note from the database, so an edit still in the
    // autosave queue would be missing from the file.
    let saved!: () => void;
    const flushEdits = vi.fn(() => new Promise<void>((resolve) => (saved = resolve)));
    const { exportSpy } = setup({}, "/Users/me/weekly-sync.md", flushEdits);
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));
    await waitFor(() => expect(flushEdits).toHaveBeenCalledTimes(1));
    expect(exportSpy).not.toHaveBeenCalled();
    saved();
    await waitFor(() => expect(exportSpy).toHaveBeenCalledTimes(1));
  });

  it("does not export a stale copy when the queued edits fail to save", async () => {
    const { exportSpy, onClose } = setup({}, "/Users/me/weekly-sync.md", () =>
      Promise.reject(new Error("database is locked")),
    );
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));
    await waitFor(() =>
      expect(useRecordingStore.getState().errors.at(-1)).toMatchObject({
        kind: "export",
        message: "Error: database is locked",
      }),
    );
    expect(exportSpy).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("carries the speaker-labels toggle and format into the spec", async () => {
    const { exportSpy } = setup();
    await userEvent.click(
      screen.getByRole("switch", { name: "Include speaker labels in transcript" }),
    );
    await userEvent.click(screen.getByRole("radio", { name: "Plain text" }));
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));
    await waitFor(() => expect(exportSpy).toHaveBeenCalledTimes(1));
    const spec = (exportSpy.mock.calls[0]![0] as { spec: ExportSpec }).spec;
    expect(spec.includeSpeakerLabels).toBe(false);
    expect(spec.format).toBe("txt");
  });

  it("offers PDF and proposes a .pdf file for it", async () => {
    const saveSpy = vi.fn((_args: unknown) => "/Users/me/weekly-sync.pdf");
    const exportSpy = vi.fn();
    mockTauri({
      "plugin:dialog|save": (args) => saveSpy(args),
      export_note: (args) => exportSpy(args),
    });
    render(
      <ExportModal
        note={makeNote()}
        open
        onClose={vi.fn()}
        flushEdits={() => Promise.resolve()}
      />,
    );
    await userEvent.click(screen.getByRole("radio", { name: "PDF" }));
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));
    await waitFor(() => expect(exportSpy).toHaveBeenCalledTimes(1));
    expect(saveSpy.mock.calls[0]![0]).toMatchObject({
      options: { defaultPath: "weekly-sync.pdf", filters: [{ extensions: ["pdf"] }] },
    });
    expect((exportSpy.mock.calls[0]![0] as { spec: ExportSpec }).spec.format).toBe("pdf");
  });

  it("selecting Notes includes it in the spec", async () => {
    const { exportSpy } = setup();
    await userEvent.click(screen.getByRole("checkbox", { name: "Notes" }));
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));
    await waitFor(() => expect(exportSpy).toHaveBeenCalledTimes(1));
    expect((exportSpy.mock.calls[0]![0] as { spec: ExportSpec }).spec.includeNotes).toBe(true);
  });

  it("does not export when the save panel is cancelled", async () => {
    const { exportSpy, onClose } = setup({}, null);
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));
    // Give the async handler a chance to run.
    await new Promise((r) => setTimeout(r, 0));
    expect(exportSpy).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("disables empty content and the export button when nothing is exportable", () => {
    setup({ summary: "", transcript: "", body: "<p></p>" });
    expect(screen.getByRole("checkbox", { name: /Summary/ })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: /Transcript/ })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: /Notes/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Export…" })).toBeDisabled();
  });
});
