// An account's CRA registration, kept in the account's `meta` under `canada`:
//   { "canada": { "registration": "TFSA", "holder": "self" } }
// The Rust side (`loontax-canada-tax`, registration.rs) reads the same shape and
// refuses unknown fields or codes, so this module writes nothing else there.

export const REGISTRATIONS = [
  "NON_REGISTERED",
  "TFSA",
  "RRSP",
  "SPOUSAL_RRSP",
  "FHSA",
  "RESP",
  "LIRA",
  "RRIF",
] as const;

export type Registration = (typeof REGISTRATIONS)[number];

export const HOLDERS = ["self", "spouse"] as const;
export type Holder = (typeof HOLDERS)[number];
export const DEFAULT_HOLDER: Holder = "self";

export interface CanadaAccountInfo {
  registration: Registration;
  holder: string;
}

/**
 * - `null`: the user has not set it. The tax report leaves such an account's
 *   securities out; it never reads it as non-registered.
 * - `"unreadable"`: something is stored that the tax report refuses.
 */
export type CanadaInfoState = CanadaAccountInfo | null | "unreadable";

const CANADA_KEY = "canada";

function asObject(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function parseMeta(meta?: string | null): Record<string, unknown> | null | "invalid" {
  if (!meta?.trim()) return null;
  try {
    return asObject(JSON.parse(meta)) ?? "invalid";
  } catch {
    return "invalid";
  }
}

function isRegistration(value: unknown): value is Registration {
  return typeof value === "string" && (REGISTRATIONS as readonly string[]).includes(value);
}

export function getCanadaInfo(meta?: string | null): CanadaInfoState {
  const parsed = parseMeta(meta);
  if (parsed === null) return null;
  if (parsed === "invalid") return "unreadable";
  if (!(CANADA_KEY in parsed)) return null;

  const entry = asObject(parsed[CANADA_KEY]);
  if (!entry) return "unreadable";
  const keys = Object.keys(entry);
  if (keys.some((key) => key !== "registration" && key !== "holder")) return "unreadable";
  if (!isRegistration(entry.registration)) return "unreadable";
  const holder = entry.holder ?? DEFAULT_HOLDER;
  if (typeof holder !== "string" || !holder.trim()) return "unreadable";
  return { registration: entry.registration, holder };
}

/** Writes or removes the `canada` entry, keeping every other entry of `meta`. */
export function setCanadaInfo(
  meta: string | null | undefined,
  info: CanadaAccountInfo | null,
): string {
  const parsed = parseMeta(meta);
  // Meta that is not a JSON object cannot hold settings: start a new one.
  const root = parsed && parsed !== "invalid" ? { ...parsed } : {};
  if (info) {
    root[CANADA_KEY] = { registration: info.registration, holder: info.holder };
  } else {
    delete root[CANADA_KEY];
  }
  return JSON.stringify(root);
}

// Broker codes as connect's `map_broker_account_type` sees them (upper-cased).
// Only registered plans are suggested: a broker's "CASH" or "MARGIN" account
// can still be a joint or corporate one, which this app does not model.
const BROKER_SUGGESTIONS: Readonly<Record<string, Registration>> = {
  TFSA: "TFSA",
  RRSP: "RRSP",
  RSP: "RRSP",
  SRRSP: "SPOUSAL_RRSP",
  SPOUSAL_RRSP: "SPOUSAL_RRSP",
  "SPOUSAL RRSP": "SPOUSAL_RRSP",
  FHSA: "FHSA",
  RESP: "RESP",
  LIRA: "LIRA",
  RRIF: "RRIF",
};

/** The registration the account's broker reports, when it is a registered plan. */
export function suggestRegistration(meta?: string | null): Registration | null {
  const parsed = parseMeta(meta);
  if (!parsed || parsed === "invalid") return null;
  const brokerType = parsed.broker_account_type;
  if (typeof brokerType !== "string") return null;
  return BROKER_SUGGESTIONS[brokerType.trim().toUpperCase()] ?? null;
}
