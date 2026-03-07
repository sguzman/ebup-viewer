import type { BridgeError, SessionState } from "../../types";
import type { ActionTelemetry, OperationScope, ToastMessage } from "../appStore";
import type { StoreGet, StoreSet } from "./types";

type ToastSeverity = "info" | "success" | "error";
const TELEMETRY_LIMIT = 200;

export function toMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const withMessage = error as { message: unknown };
    if (typeof withMessage.message === "string") {
      return withMessage.message;
    }
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function toBridgeError(error: unknown): BridgeError {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    const structured = error as { code: unknown; message: unknown };
    if (typeof structured.code === "string" && typeof structured.message === "string") {
      return {
        code: structured.code,
        message: structured.message
      };
    }
  }
  return {
    code: "unknown_error",
    message: toMessage(error)
  };
}

export function buildToast(severity: ToastSeverity, message: string): ToastMessage {
  return {
    id: Date.now(),
    severity,
    message
  };
}

export function appendTelemetry(
  set: StoreSet,
  get: StoreGet,
  telemetry: ActionTelemetry
): void {
  const next = [telemetry, ...get().telemetry];
  if (next.length > TELEMETRY_LIMIT) {
    next.length = TELEMETRY_LIMIT;
  }
  set({ telemetry: next });
}

export function finishTelemetry(
  set: StoreSet,
  get: StoreGet,
  action: string,
  startedAt: number,
  ok: boolean,
  error: string | null
): void {
  appendTelemetry(set, get, {
    id: Date.now(),
    action,
    started_at_unix_ms: startedAt,
    duration_ms: Date.now() - startedAt,
    ok,
    error
  });
}

export function togglePanels(
  panels: SessionState["panels"],
  panel: "show_settings" | "show_stats" | "show_tts"
): SessionState["panels"] {
  const next = {
    ...panels,
    [panel]: !panels[panel]
  };
  if (panel === "show_settings" && next.show_settings) {
    next.show_stats = false;
  }
  if (panel === "show_stats" && next.show_stats) {
    next.show_settings = false;
  }
  return next;
}

export function setOperationBusy(
  set: StoreSet,
  get: StoreGet,
  scope: OperationScope,
  active: boolean
): void {
  const current = get().operations;
  const next = {
    ...current,
    [scope]: active
  };
  const busy =
    next.sourceOpen ||
    next.starterCommand ||
    next.readerCommand ||
    next.readerTts ||
    next.readerSettings ||
    next.browserTabRefresh ||
    next.calibreLoad ||
    next.runtimeConfig;
  set({
    operations: next,
    busy
  });
}

export async function withBusy(
  set: StoreSet,
  get: StoreGet,
  action: string,
  scope: OperationScope,
  fn: () => Promise<void>
): Promise<void> {
  const startedAt = Date.now();
  set({ error: null });
  setOperationBusy(set, get, scope, true);
  try {
    await fn();
    finishTelemetry(set, get, action, startedAt, true, null);
  } catch (error) {
    finishTelemetry(set, get, action, startedAt, false, toMessage(error));
  } finally {
    setOperationBusy(set, get, scope, false);
  }
}
