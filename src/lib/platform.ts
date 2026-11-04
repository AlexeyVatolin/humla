import type { PermissionStatus } from "./ipc";

export const isWindows = /Windows/i.test(navigator.userAgent);
export const systemSettingsName = isWindows ? "Windows Settings" : "System Settings";
export const trayName = isWindows ? "system tray" : "menu bar";
export const primaryModifier = isWindows ? "Ctrl+" : "⌘";
export const credentialStoreName = isWindows ? "Windows Credential Manager" : "macOS Keychain";

export function permissionReady(status: PermissionStatus | undefined): boolean {
  return status === "granted" || status === "not_required";
}
