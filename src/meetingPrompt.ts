import { invoke } from "@tauri-apps/api/core";
import "./meetingPrompt.css";

const record = document.querySelector<HTMLButtonElement>("#record")!;
const dismiss = document.querySelector<HTMLButtonElement>("#dismiss")!;
const message = document.querySelector<HTMLParagraphElement>("#message")!;

dismiss.addEventListener("click", () => void invoke("meeting_prompt_dismiss"));
record.addEventListener("click", async () => {
  record.disabled = true;
  try {
    await invoke("meeting_prompt_start");
  } catch (error) {
    message.textContent = String(error);
    record.disabled = false;
  }
});
