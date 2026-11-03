import { LANGUAGES } from "../../../lib/languages";
import type { LocalState } from "../types";
import { ModelDownloadCard } from "../../../components/provider/ModelDownloadCard";

function languageLabel(code: string | null): string {
  if (!code) return "Unknown";
  const found = LANGUAGES.find((l) => l.value === code);
  return found?.label ?? code;
}

export function LocalModelManager({
  state,
  activeId,
  onDownload,
  onDelete,
  onSelect,
}: {
  state: LocalState;
  activeId: string;
  // Kept for call-shape compatibility; the list no longer reads it.
  language?: string;
  onDownload: (id: string) => void;
  onDelete: (id: string) => void;
  onSelect: (id: string) => void;
}) {
  return (
    <>
      <p className="text-xs text-[var(--color-text-muted)] py-3">
        Choose a local model for transcription.
      </p>
      {state.models.map((m) => {
        const isActive = m.id === activeId;
        return (
          <ModelDownloadCard
            key={m.id}
            model={m}
            tag={
              m.kind === "multilingual"
                ? "Multilingual"
                : languageLabel(m.specificLanguage)
            }
            selected={isActive}
            onSelect={() => onSelect(m.id)}
            onDownload={onDownload}
            onDelete={onDelete}
            warning={
              // The default's model_id can point at a model that isn't on
              // disk (fresh installs; onboarding skips). Recording with the
              // local provider won't start in that state — say so here,
              // where the user is already looking, instead of letting
              // Record fail.
              isActive && !m.downloaded
                ? "Selected as the default but not downloaded — recording won't start until you download it."
                : undefined
            }
          />
        );
      })}
      {state.flash && (
        <div
          className="flex items-center gap-3 px-3 py-2 my-3 rounded bg-[var(--color-pill-hover)] text-xs"
          role="status"
        >
          <span className="break-all">{state.flash.message}</span>
        </div>
      )}
      {state.error && (
        <p className="text-sm text-[var(--color-danger)] break-all py-3">
          {state.error}
        </p>
      )}
    </>
  );
}
