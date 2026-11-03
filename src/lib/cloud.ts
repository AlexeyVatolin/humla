import { create } from "zustand";
import type { SyncStatus } from "./ipc";

export type CloudRole = "owner" | "admin" | "member" | "viewer";
export type PlanStatus = "active" | "trialing" | "past_due" | "canceled" | "none";

export type CloudUser = { id: string; email: string; name: string; verified: boolean };
export type CloudWorkspace = {
  id: string;
  name: string;
  role: CloudRole;
  plan_status: PlanStatus;
  /** Billed seats (= workspace members). Absent on servers predating per-seat billing. */
  seats?: number | null;
  /** Every member must have two-factor authentication to open it. */
  require_2fa?: boolean;
};

/** A workspace named and nothing more: all a member is told about one that
 *  requires two-factor authentication they don't have yet. */
export type WorkspaceRef = { id: string; name: string };

/** The signed-in user's two-factor authentication. `available` is false
 *  on a server without it, and the app then shows none of it. */
export type TwoFactorStatus = {
  available: boolean;
  enabled: boolean;
  recovery_codes_left: number;
  /** Every workspace the user is in that requires it. */
  required_by: WorkspaceRef[];
};

export const TWO_FACTOR_UNAVAILABLE: TwoFactorStatus = {
  available: false,
  enabled: false,
  recovery_codes_left: 0,
  required_by: [],
};

/** Where a password sign-in landed: signed in, or waiting for the code from
 *  the user's authenticator app (`cloudApi.verifyTwoFactor`). */
export type SignInStep =
  | { step: "signed_in"; status: CloudStatus }
  | { step: "two_factor" };

/** The second factor a two-factor route takes. */
export type SecondFactor = { code: string } | { recoveryCode: string };

/** What an authenticator app is set up with. `qr.path` draws its dark modules
 *  one unit apart; the quiet zone is the page's to add. */
export type TwoFactorSetup = {
  secret: string;
  otpauth_uri: string;
  qr: { size: number; path: string } | null;
};

/** How the two-factor commands fail: `reason` is the server's code to branch
 *  on (empty when there is none), `message` what the user reads. */
export type CloudError = { reason: string; message: string };
export type CloudMember = { id: string; email: string; name: string; role: CloudRole };

/**
 * Workspace chat-key metadata (BYOK, issue #75). The key value is never exposed
 * — only this metadata. `keyHealth` ("ok" | "failing" | …) drives the
 * degradation warning; `setBy` is a user id resolved to a name in the UI.
 */
export type ChatKeyMeta = {
  configured: boolean;
  last4: string | null;
  setBy: string | null;
  setAt: string | null;
  keyHealth: string | null;
};

export type CloudStatus = {
  /** A server URL is configured. */
  configured: boolean;
  /** We hold (or could auto-acquire) a valid session. */
  logged_in: boolean;
  base_url: string;
  user: CloudUser | null;
  current_workspace: CloudWorkspace | null;
  workspaces: CloudWorkspace[];
  /** True when the server enforces billing (humla-cloud); false on self-host. */
  billing_enabled: boolean;
  /** Per-seat price in cents (Stripe unit_amount, e.g. 500 = $5.00). Absent on older servers. */
  seat_price_cents?: number | null;
  /** Lowercase ISO currency for seat_price_cents (e.g. "usd"). Absent when unknown. */
  seat_currency?: string | null;
  /** Managed chat add-on config, when the server advertises it (issue #75).
   *  null/absent → self-host or not configured; the add-on pitch is dropped. */
  chat_addon?: ChatAddon | null;
  /** The signed-in user's two-factor authentication; unavailable when signed out. */
  two_factor?: TwoFactorStatus;
  /** Workspaces the user is in but can't open until they set up two-factor
   *  authentication: the server leaves them out of `workspaces`. */
  locked_workspaces?: WorkspaceRef[];
  /** The selected workspace when it is one of `locked_workspaces` — then
   *  `current_workspace` is null although a workspace is selected. */
  locked_current_workspace?: WorkspaceRef | null;
  /** Signed out without signing out: the email of the account whose session
   *  ended, for the sign-in form to ask them back. */
  signed_out_email?: string | null;
};

/** Managed chat add-on advertised in the server's billing config (issue #75). */
export type ChatAddon = {
  available: boolean;
  price_id?: string | null;
  price_cents?: number | null;
  currency?: string | null;
};

export const DISCONNECTED: CloudStatus = {
  configured: false,
  logged_in: false,
  base_url: "",
  user: null,
  current_workspace: null,
  workspaces: [],
  billing_enabled: false,
  seat_price_cents: null,
  seat_currency: null,
  chat_addon: null,
  two_factor: TWO_FACTOR_UNAVAILABLE,
  locked_workspaces: [],
  locked_current_workspace: null,
  signed_out_email: null,
};

export const useCloudStore = create(() => ({
  status: DISCONNECTED, ready: true, members: {} as Record<string, CloudMember>,
  syncStatus: null as SyncStatus | null, pendingNoteIds: new Set<string>(),
}));

/**
 * Resolve a note's `owner` id to a display name for "created by" attribution,
 * using the active workspace's member list. Returns null when the note is
 * yours, has no owner, or the owner can't be resolved (e.g. a former member) —
 * callers should render nothing in those cases.
 */
export function useOwnerName(ownerId: string | undefined | null): string | null {
  const me = useCloudStore((s) => s.status.user?.id);
  const member = useCloudStore((s) => (ownerId ? s.members[ownerId] : undefined));
  const rosterLoaded = useCloudStore((s) => Object.keys(s.members).length > 0);
  if (!ownerId || ownerId === me) return null;
  if (member) return member.name || member.email;
  // Owner isn't in the current roster. If the roster is loaded, they've left the
  // workspace — label it rather than rendering nothing (which would read as
  // "yours"). If the roster isn't loaded yet, don't guess.
  return rosterLoaded ? "Former member" : null;
}

/**
 * Resolve a workspace member id to their display name — plainly, with none of
 * `useOwnerName`'s attribution logic. That one collapses "it's you" and "can't
 * resolve them" to the same `null`, which is right for a "created by" byline
 * (both render nothing) and wrong everywhere the two must be told apart: the
 * authorship chip labels a self-pin "Created by me" and an unresolvable one
 * neutrally, and the chat turn sends the name for the model's disclosure line
 * whether or not the pinned person is the caller (#103).
 */
export function useMemberName(id: string | undefined | null): string | null {
  const member = useCloudStore((s) => (id ? s.members[id] : undefined));
  const myName = useCloudStore((s) => (id && s.status.user?.id === id ? s.status.user : null));
  if (!id) return null;
  if (member) return member.name || member.email || null;
  // The roster excludes the caller on some servers, so fall back to their own
  // account before giving up — a self-pin must not read as a stranger.
  return myName ? myName.name || myName.email || null : null;
}
